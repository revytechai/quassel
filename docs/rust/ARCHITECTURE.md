# The architecture of the Rust core

The Rust port lives under `rust/`, as a Cargo workspace of five crates.  The
idea is the same as in the C++ tree: the core is the daemon, the clients are
windows on it, and everything that both sides need to agree on — the wire
format, the storage, the IRC protocol — sits in a crate of its own.

```
rust/
├── Cargo.toml              the workspace
├── Makefile                the targets
├── README.md               for the people who run it
├── man/quasselcore-rs.1    the manual page
└── crates/
    ├── qs-datastream/      the QDataStream/QVariant codec, no I/O
    ├── qs-proto/           the handshake and the signal proxy layer
    ├── qs-storage/         SQLite and PostgreSQL, schema 32
    ├── qs-irc/             the IRC connections over TLS
    └── quasselcore-rs/     the binary
```

The crates depend on each other in one direction only:

```
                 quasselcore-rs
                 /     |      \
          qs-datastream  |   qs-storage
                 \       |        /
                  qs-proto     qs-irc
```

`qs-datastream` knows nothing about the protocol, the storage or the
network; `qs-proto` knows nothing about the storage; `qs-storage` and
`qs-irc` know nothing about each other.  The binary ties them together.

## `qs-datastream` — the bytes

The Quassel protocol is a Qt flavour of `QDataStream`.  Every value on the
wire is a `QVariant`: a 32 bit type id, an 8 bit null flag, and a type
specific payload.  This crate turns values into bytes and back, and does
nothing else — no sockets, no files, no clock.

The shapes that are easy to get wrong, and that the crate therefore makes
explicit:

* A **message body** is a *list body*: a 32 bit element count followed by the
  elements.  There is **no** `QVariant` type header in front of the count.
  Writing a `QVariant`-wrapped list makes the core read the element count
  from the type field and silently ignore the message.
* A **map key** and a **string list element** are plain `QString` bodies (a
  byte count and UTF-16BE); only a map's value carries the `QVariant`
  header.
* A **null** `QString` or `QByteArray` is the length `0xffffffff` and nothing
  else.  The null flag is read by the core and thrown away; the null-ness
  comes from the length.
* A **user type** is the type id 127, then the type name (a length that
  counts the NUL terminator, the name, the NUL), then the payload.  The core
  rejects names it does not know, and the server list type is called
  `Network::Server` — **not** `Network_Server`.

The crate has:

* `Value`, the value model, with `Map` (key sorted, like Qt's `QVariantMap`)
  and `UserType` for the Quassel specific shapes;
* `encode_value`, `encode_list_body` and their inverses;
* property tests (`proptest`) that check that whatever goes in comes out
  again, and that the encoding is canonical;
* a **fuzz target** (`crates/qs-datastream/fuzz`) that feeds the decoder
  whatever bytes the fuzzer makes up, and checks that it never panics and
  that whatever it accepts it can also write back.

## `qs-proto` — what the bytes mean

The crate has three parts.

**The handshake** (`handshake.rs`).  The client opens a plain TCP connection
and sends

```
uint32  0x42b33f00 | the connection features it wants
uint32  the protocols it speaks, the last one with bit 31 set
```

and the core answers with a single 32 bit word: the protocol it picked, the
protocol features it enabled, and the connection features it offers
(`0x01` = encryption, `0x02` = compression).

**The handshake messages** (`messages.rs`): `ClientInit`, `ClientInitAck`,
`ClientLogin`, `ClientLoginAck`, `SessionInit` and the setup messages.  A
handshake message is a flat list of alternating byte array keys and values.

**The signal proxy layer** (`signal.rs`): the wire shapes of the
synchronised objects — a sync call (class name, object name, slot name,
arguments), an RPC call (signal name, arguments), an init request and its
answer, and the heartbeats.

`framing.rs` has the message framing: a 32 bit length in front of every
message, and a decoder that puts the bytes back together.

The crate has no I/O either: it produces and consumes byte slices.

## `qs-storage` — where the data lives

Two backends, **SQLite** (the default; a file in the configuration
directory) and **PostgreSQL**, both on `sqlx`.

Both read and migrate the databases the C++ core writes, unchanged:

* the schema is the one from `src/core/SQL/`, copied into
  `crates/qs-storage/sql/` — the test `the_sql_matches_the_cores` checks that
  the copy has not drifted from the C++ tree;
* the schema version lives in the `coreinfo` table, exactly as the core keeps
  it (32 for SQLite, 31 for PostgreSQL);
* the *setup* statements create the schema of a fresh database, and the
  *upgrade* statements in `version/##` bring an older database up to date,
  one version at a time.

**The empty codec rule.**  The `network` table's three codec columns are
`NOT NULL`.  The C++ core binds whatever the client sent, and a client that
leaves "use custom encodings" off sends a *null* byte array, which Qt turns
into a null string, which the SQLite driver binds as SQL NULL — and the
insert fails with

```
NOT NULL constraint failed: network.encodingcodec
```

This crate never writes SQL NULL into those columns: a codec that is not
there, or that is empty, is stored as an empty string.  The test
`a_network_with_empty_codecs_stores_empty_strings_and_never_null` checks
the stored value's *type* as well as its content.

**Passwords** are hashed the way the core hashes them: a salted SHA-512,
stored as `sha512_hex(password + salt) + ":" + salt`.  A database that still
holds the old SHA-1 hashes is recognised and upgraded on the first successful
login.

## `qs-irc` — the IRC connections

`tokio` for the network, `rustls` for TLS.

The certificate of an IRC server is checked against the system's roots.  A
server with a self-signed certificate would fail that check, and the C++ core
answers such a server by turning the verification off — which means anybody
in the middle could impersonate the server.

This crate adds a third option: **pin the SHA-256 fingerprint** of the
certificate.  The certificate then has to be exactly the one that was pinned
down, and the verification stays on.  The fingerprint is the SHA-256 of the
DER encoded certificate, written as 64 hex characters, the way the IRCv3
`certfp` specification writes it.

The crate also holds the reconnect policy: how often the core tries again,
and how many times it gives up.

## `quasselcore-rs` — the binary

The binary is a thin layer around the library of the same name:

* `cli.rs` — the command line, with the flags the C++ core has;
* `config.rs` — `quasselcore.conf`, in the INI format Qt's `QSettings`
  writes;
* `logging.rs` — the log levels the C++ core has;
* `server.rs` — the listener, the handshake, the TLS upgrade;
* `session.rs` — one client: the handshake messages, the login, and the
  state of the session.

### The flags

The flags the C++ core has, with the same names and the same meaning:

```
-c, --configdir <path>          the configuration directory
    --listen <address>          the addresses to listen on (default ::,0.0.0.0)
-p, --port <port>               the port to listen at (default 4242)
-n, --norestore                 reserved: the core does not restore the state
    --select-backend <name>     SQLite or PostgreSQL, for this run
    --select-authenticator <id> the authenticator to use (Database only)
    --add-user                  add a user
    --change-userpass <name>    change a user's password
    --user <name>               the user name for --add-user
    --password-file <path>      read the password from this file
    --require-tls, --require-ssl
    --tls-cert, --tls-key       the certificate and the key
    --ssl-cert, --ssl-key       the old names of the two flags
    --strict-ident               reserved: the core refuses the option
-L, --loglevel <level>          Debug, Info, Warning or Error
-l, --logfile <path>
-d, --debug
```

The password is **never** taken from the command line, where every other
process could read it out of the process list.  It comes from the terminal
(the C++ core's way) or from a file.

### The configuration file

The core reads the same file the C++ core reads, `quasselcore.conf` in the
configuration directory:

```ini
[Core]
StorageSettings\Backend=SQLite
StorageSettings\ConnectionProperties\Database=quassel-storage.sqlite
AuthSettings\Authenticator=Database

[Core\SSL]
Certificate=/var/db/quasselcore/quasselCert.pem
Key=/var/db/quasselcore/quasselCert.pem
SystemCA=true
```

A key of a sub-group is written with a backslash, exactly as Qt writes it.

The certificate is one PEM file that holds the private key and the
certificate, concatenated — the layout of `quasselCert.pem` the C++ core
uses.

### What a client sees

1. The client sends the handshake; the core answers with the protocol it
   picked and the connection features it offers.
2. The connection becomes a TLS one — the core does **not** start a TLS
   handshake when the socket opens, it reads the plaintext handshake first.
3. The client sends `ClientInit`; the core answers with `ClientInitAck`, with
   the features it supports and the storage backends it offers.
4. The client sends `ClientLogin`; the core answers with `ClientLoginAck`, or
   with `ClientLoginReject` and the reason.
5. The core sends `SessionInit`: the user's identities, buffers and networks.
6. From then on the two sides talk the signal proxy protocol.

## The plan of the later rounds

The order is **the core first, a client afterwards**.  A client is only
worth writing when the core keeps its state properly.

### Round 2 — the core's live state

* the synchronised objects: `CoreSession`, `CoreIdentity`, `CoreNetwork`,
  `CoreBufferSyncer`, `CoreBacklogManager`, `CoreHighlightRuleManager`,
  `CoreIgnoreListManager`, `CoreAliasManager`, `CoreBufferViewManager`,
  `CoreBufferViewConfig`, `CoreTransferManager`;
* the signal proxy on the core's side: the objects, their slots, and the
  init data a client asks for;
* `createNetwork`, `removeNetwork`, `requestConnect` and the rest of the
  network handling, including the empty codec rule end to end.

### Round 3 — the IRC side

* the IRC parser (`src/core/ircparser.cpp`) and the event stringifier
  (`src/core/eventstringifier.cpp`);
* the IRC connection: the CAP negotiation, the SASL, the away handling, the
  modes, the channels, the users, the netsplits;
* the backlog: the messages, the storage of them, the filters;
* reconnecting the networks that were connected when the core stopped, with
  the fingerprint pinning of `qs-irc`.

### Round 4 — the core's edges

* the ident daemon, the metrics server, the LDAP authenticator, the message
  encryption;
* the setup path: a core that is not configured yet, and the client that sets
  it up;
* the `--select-backend` migration between SQLite and PostgreSQL.

### Round 5 — a client

* the client's own logic as a `qs-client` crate: the connection, the models,
  the buffer views;
* the user interface, in a toolkit of its own — not Qt, so that the port does
  not drag in a C++ build.

### Round 6 — the packaging

* a FreeBSD port, an rc script, the `quasselcore.conf` of the port;
* the man pages;
* the release notes.

## How the port is checked

| what | how |
|---|---|
| the codec | property tests (`proptest`) and a fuzz target that never panics |
| the protocol | tests against the byte strings the C++ core and `quassel-client.py` put on the wire |
| the storage | tests on a database created with the core's own DDL, including the migration of an old one and the empty codec rule |
| the IRC layer | tests against a TLS server with a self-signed certificate: refused without a pin, accepted with the right pin |
| the core | an integration test that does the whole handshake and login, and the scripted client `quassel-client.py` against the running binary |
