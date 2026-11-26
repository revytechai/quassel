# The Quassel tree, and where it goes in Rust

This is the inventory of the C++ tree that the Rust port replaces, and the
crate each part of it ends up in.

The order is deliberate: **the core comes first, the GUI client last.**  The
core is what holds the IRC connections and the chat history; a client is a
window on the core's state.  Porting the core first means the protocol, the
storage and the IRC handling are settled before any user interface is built
on top of them.

The numbers are lines of C++ (`.cpp` and `.h`) as of the `rust/main` branch
of `revytechai/quassel`.

## The tree at a glance

| directory | files | lines | what it is | the crate it becomes |
|---|---:|---:|---|---|
| `src/core` | 91 | 24 889 | the core: storage, IRC, sessions | `quasselcore-rs`, `qs-storage`, `qs-irc` |
| `src/common` | 113 | 22 277 | what the core and the client share: the protocol, the data types | `qs-datastream`, `qs-proto`, and later `qs-model` |
| `src/client` | 61 | 11 311 | the client's own logic, without any user interface | a future `qs-client` |
| `src/uisupport` | 64 | 12 819 | the widgets that are not Quassel-specific | stays in C++ |
| `src/qtui` | 174 | 30 711 | the Qt user interface | stays in C++ |
| `src/main` | 3 | 294 | the entry points of the three programs | `quasselcore-rs` |
| `src/test` | 8 | 776 | the test helpers of the C++ tree | replaced by the Rust tests |
| **total** | **514** | **103 077** | | |

Roughly a third of the tree is the user interface, and the user interface is
the part that is *not* ported: the plan is a new client, not a Qt one.

## `src/core` — the core (91 files, 24 889 lines)

The core is the daemon.  It keeps the IRC connections, the chat history and
the users, and it serves the clients.

### The storage (13 files, 7 700 lines)

| file | lines | what it does |
|---|---:|---|
| `abstractsqlstorage.cpp/.h` | 1 100 | the schema: which version a database has, how it is set up, how it is upgraded |
| `sqlitestorage.cpp/.h` | 2 845 | the SQLite backend |
| `postgresqlstorage.cpp/.h` | 2 765 | the PostgreSQL backend |
| `storage.cpp/.h` | 704 | the interface, and the password hashing |
| `coresettings.cpp/.h` | 103 | `quasselcore.conf` |
| `sqlauthenticator.cpp/.h` | 128 | the "Database" authenticator |

**Goes to `qs-storage`.**  The schema itself (the SQL under `src/core/SQL`)
stays what it is: the Rust storage reads and writes the same databases, so a
database can be moved from the C++ core to the Rust core and back.

### The server and the sessions (24 files, 7 600 lines)

| file | lines | what it does |
|---|---:|---|
| `core.cpp/.h` | 2 070 | the core itself: the listener, the setup, the users |
| `coresession.cpp/.h` | 1 132 | one session per user: the state, the settings, the networks |
| `coresessioneventprocessor.cpp/.h` | 1 866 | what happens when an IRC event comes in |
| `coreauthhandler.cpp/.h` | 421 | the handshake and the login |
| `sessionthread.cpp/.h` | 213 | one thread per session |
| `coreapplication.cpp/.h` | 90 | the `QCoreApplication` of the core |
| `corenetwork.cpp/.h` | 2 331 | one IRC network: the connection, the channels, the modes |
| `coreuserinputhandler.cpp/.h` | 1 225 | what the user types, and what the core does with it |
| `sslserver.cpp/.h` | 324 | the TLS server and the certificate |
| `identserver.cpp/.h` | 391 | the ident daemon |
| `metricsserver.cpp/.h` | 468 | the Prometheus metrics |
| `cipher.cpp/.h` | 565 | the message encryption |
| `ircparser.cpp/.h`, `ctcpparser.cpp/.h` | 1 077 | parsing the IRC protocol |

**Goes to `quasselcore-rs` (the server, the sessions, the parser) and
`qs-irc` (the IRC connections).**

### The synchronised objects (22 files, 3 600 lines)

`corealiasmanager`, `corebacklogmanager`, `corebasichandler`,
`corebuffersyncer`, `corebufferviewconfig`, `corebufferviewmanager`,
`corehighlightrulemanager`, `coreidentity`, `coreignorelistmanager`,
`coreircchannel`, `coreircuser`, `coretransfer`, `coretransfermanager`,
`coredccconfig`, `coreirclisthelper`, `coreeventmanager`, `corealiasmanager`.

**Goes to `quasselcore-rs`**, in the rounds that come after this one.  Each
of them is a *sync object*: the client holds a copy, the core holds the
master, and the two are kept in step by the signal proxy.

### The rest

| file | lines | what it does |
|---|---:|---|
| `ldapauthenticator.cpp/.h` | 392 | the LDAP authenticator |
| `eventstringifier.cpp/.h` | 1 028 | turns IRC events into the text the client shows |
| `netsplit.cpp/.h` | 327 | the `host!user@server` parser |
| `keyevent.cpp/.h` | 127 | the key codes |
| `oidentdconfiggenerator.cpp/.h` | 270 | writes the oidentd configuration |
| `backgroundtaskhandler.cpp/.h` | 44 | runs a task in the background |

## `src/common` — what the core and the client share (113 files, 22 277 lines)

### The protocol (12 files, 3 700 lines)

| file | lines | what it does |
|---|---:|---|
| `protocol.h` | 272 | the message types of the protocol |
| `protocols/datastream/datastreampeer.cpp/.h` | 503 | the protocol everybody uses |
| `protocols/legacy/legacypeer.cpp/.h` | 645 | the old Qt protocol |
| `serializers/serializers.cpp/.h` | 705 | turns the bytes into values and back |
| `signalproxy.cpp/.h` | 1 370 | the signal proxy: the sync objects, the RPC calls |
| `remotepeer.cpp/.h` | 428 | the transport of a remote peer |
| `peer.cpp/.h`, `peerfactory.cpp/.h` | 276 | picks the protocol |
| `compressor.cpp/.h` | 347 | the compression of the messages |
| `proxyline.cpp/.h` | 109 | the proxy protocol line |

**Goes to `qs-datastream` (the codec) and `qs-proto` (the handshake, the
signal proxy layer).**

### The data types (26 files, 7 200 lines)

`network.cpp/.h` (2 205 lines) is the big one: the network, its servers, the
network info that travels over the wire.  `identity.cpp/.h` (548),
`message.cpp/.h` (303), `ircchannel.cpp/.h` (749), `ircuser.cpp/.h` (659),
`bufferinfo.cpp/.h` (180), `types.h` (254), `irccap.h` (212),
`ircencoder`/`ircdecoder` (460), `ircevent.cpp/.h` (230), `irctag(s).h` (150),
`irclisthelper` (91), `ctcpevent` (152), `event.cpp/.h` (217),
`messageevent` (176), `networkevent` (239), `networkconfig` (208),
`presetnetworks` (132).

**Goes to a future `qs-model` crate.**  For this round only the shapes that
cross the wire are in Rust, in `qs-datastream`.

### The settings and the managers (30 files, 7 500 lines)

`settings.cpp/.h` (481) reads the settings files; the managers
(`aliasmanager`, `backlogmanager`, `bufferviewmanager`, `buffersyncer`,
`bufferviewconfig`, `highlightrulemanager`, `ignorelistmanager`,
`transfermanager`, `eventmanager`, `coreinfo`) hold the state that is
synchronised with the client.

**Goes to `quasselcore-rs`** in the later rounds.

### The rest

`quassel.cpp/.h` (1 081) holds the build information, the feature list and
the command line; `util.cpp/.h` (506) the small helpers; `logger.cpp/.h`
(376); `syncableobject.cpp/.h` (307), the base of every synchronised
object; `expressionmatch.cpp/.h` (1 290), the highlighting rules;
`internalpeer.cpp/.h` (303), the connection inside one process;
`transfer.cpp/.h` (375); `funchelpers.h` (168); `singleton.h` (129);
`deferredptr.h` (100); the signal watchers (240).

## `src/client` — the client's own logic (61 files, 11 311 lines)

The part of the client that is not Qt: the models and the connection.

| file | lines | what it does |
|---|---:|---|
| `client.cpp/.h` | 1 011 | the client itself |
| `networkmodel.cpp/.h` | 2 090 | the model of the networks |
| `clientauthhandler.cpp/.h` | 673 | the client's side of the handshake |
| `coreconnection.cpp/.h` | 698 | the connection to the core |
| `treemodel.cpp/.h` | 833 | the tree the buffer views use |
| `messagemodel.cpp/.h` | 732 | the model of the messages |
| `clientbacklogmanager.cpp/.h` | 260 | asks the core for the backlog |
| `bufferviewoverlay.cpp/.h` | 405 | the overlay of a buffer view |
| `messagefilter.cpp/.h` | 331 | filters the messages |
| `coreaccountmodel.cpp/.h` | 337 | the accounts |
| `execwrapper.cpp/.h` | 206 | runs a command |
| the rest | 2 734 | the settings, the managers, the transfers |

**Goes to a future `qs-client` crate**, after the core is complete.

## `src/qtui` — the Qt user interface (174 files, 30 711 lines)

The window, the chat view, the input line, the settings pages and the
notification backends.  **This is not ported.**  The plan is a new client
(see `ARCHITECTURE.md`), not a port of the Qt one.

The biggest files: `mainwin.cpp` (1 971), `networkssettingspage.cpp` (1 457),
`chatscene.cpp` (1 342), `chatitem.cpp` (957), `highlightsettingspage.cpp`
(785), `corehighlightsettingspage.cpp` (785), `inputwidget.cpp` (712).

## `src/uisupport` — the widgets that are not Quassel's (64 files, 12 819 lines)

`uistyle.cpp` (1 255), `qssparser.cpp` (803), `multilineedit.cpp` (800),
`bufferview.cpp` (893), `flatproxymodel.cpp` (695), `networkmodelcontroller.cpp`
(599), `bufferviewfilter.cpp` (568), `contextmenuactionprovider.cpp` (508).

**Not ported.**  A new client picks its own toolkit.

## `src/main` — the entry points (3 files, 294 lines)

`main.cpp` (120) picks the mode — core, client or monolithic — and
`monoapplication.cpp/.h` (174) is the monolithic client's entry point.

**Goes to `quasselcore-rs`.**  The binary has one mode: the core.

## `src/test` — the C++ test helpers (8 files, 776 lines)

`mockedpeer.cpp/.h` (402) fakes a peer, `invocationspy.cpp/.h` (206) records
the calls, `printers.cpp/.h` (92) prints the values.

**Replaced** by the Rust tests: `qs-datastream`'s property tests and fuzz
target, `qs-storage`'s schema tests, `qs-proto`'s protocol tests and
`quasselcore-rs`'s handshake and login test.

## What is in Rust now

| crate | what it holds | from the C++ tree |
|---|---|---|
| `qs-datastream` | the `QDataStream`/`QVariant` codec, no I/O | `src/common/serializers/` |
| `qs-proto` | the handshake, the feature negotiation, the signal proxy layer | `src/common/protocol.h`, `src/common/protocols/`, `src/common/signalproxy.cpp` |
| `qs-storage` | the SQLite and PostgreSQL backends, schema 32 | `src/core/abstractsqlstorage.cpp`, `sqlitestorage.cpp`, `postgresqlstorage.cpp`, `src/core/SQL/` |
| `qs-irc` | the IRC connections over TLS, with fingerprint pinning | `src/core/corenetwork.cpp`, `src/core/sslserver.cpp` |
| `quasselcore-rs` | the binary: the listener, the handshake, the login, the session state | `src/core/core.cpp`, `coreauthhandler.cpp`, `coresession.cpp`, `src/main/main.cpp` |
