# quasselcore-rs — the Quassel core, in Rust

This is the core of [Quassel IRC](https://quassel-irc.org), written in Rust.
The core is the daemon that holds the IRC connections and the chat history;
the clients connect to it and talk the Quassel protocol to it.  Because the
state lives in the core, a client can be closed and opened again without
losing anything, and several clients can watch the same conversation.

The core speaks the same handshake and the same login as the C++ core, and it
reads and writes the same databases, so the existing clients can log in and
see their chats with it.  The live state — the networks, the channels and
the messages that come in while the core runs — is not implemented yet: the
core logs the signal messages of a client instead of answering them.

## Building

```sh
make            # build the workspace
make release    # build the core, ready to run
make test       # run the tests
make check      # check the formatting and run clippy
```

The build needs a Rust toolchain (1.80 or newer) and a C compiler; the
SQLite and PostgreSQL drivers are built from source.  Everything else is
pure Rust.

## Running

The core needs two things: a directory to keep its files in, and a user.
A certificate is optional; without one, the clients connect without
encryption.

```sh
# The directory holds quasselcore.conf, quassel-storage.sqlite and
# quasselCert.pem.
mkdir -p /var/db/quasselcore

# A user.  The password is asked for on the terminal, and it is never
# taken from the command line.
quasselcore-rs --configdir=/var/db/quasselcore --add-user

# The certificate: the private key and the certificate in one file.
openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout /var/db/quasselcore/quasselCert.pem \
    -out /tmp/quasselCert.pem -days 3650 -subj "/CN=quassel.example.org"
cat /tmp/quasselCert.pem >> /var/db/quasselcore/quasselCert.pem
chmod 600 /var/db/quasselcore/quasselCert.pem

# And the core itself.
quasselcore-rs --configdir=/var/db/quasselcore --listen 0.0.0.0 --port 4242
```

The core listens on port 4242 by default, on every address.  `--listen`
takes the addresses to listen on, `--port` the port.

## The configuration file

The core reads `quasselcore.conf` from its configuration directory, in the
same format the C++ core uses:

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

The storage backend is either `SQLite` (a file in the configuration
directory) or `PostgreSQL` (the connection properties are in the file, under
`StorageSettings\ConnectionProperties`).

## The command line

```
-c, --configdir <path>          the configuration directory
    --listen <address>          the addresses to listen on
-p, --port <port>               the port to listen at
-n, --norestore                 do not restore the last core's state
    --select-backend <name>     SQLite or PostgreSQL
    --select-authenticator <id> the authenticator to use
    --add-user                  add a user
    --change-userpass <name>    change a user's password
    --user <name>               the user name for --add-user
    --password-file <path>      read the password from this file
    --require-tls, --require-ssl
    --tls-cert, --tls-key       the certificate and the key
    --ssl-cert, --ssl-key       the old names of the two flags
    --strict-ident
-L, --loglevel <level>          Debug, Info, Warning or Error
-l, --logfile <path>
-d, --debug
```

`--add-user` asks for the user name and the password on the terminal, the
way the C++ core does.  With `--user` and `--password-file` the password
comes from a file, which is what a script wants; it is never taken from the
command line, where any other process could read it.

## Running it as a service

The core runs in the foreground and takes its configuration from the command
line, so an `rc.d` script is short:

```sh
#!/bin/sh
#
# PROVIDE: quasselcore
# REQUIRE: LOGIN
# KEYWORD: shutdown
#
# quasselcore_enable="YES"
# quasselcore_data="/var/db/quasselcore"
# quasselcore_log="/var/log/quasselcore.log"
# quasselcore_listen="0.0.0.0,::"
# quasselcore_user="quasselcore"
# quasselcore_group="quasselcore"
# quasselcore_args="--configdir=/var/db/quasselcore --logfile=/var/log/quasselcore.log"

. /etc/rc.subr

name="quasselcore"
rcvar="quasselcore_enable"

load_rc_config $name

: ${quasselcore_enable:="NO"}
: ${quasselcore_user:="quasselcore"}
: ${quasselcore_group:="quasselcore"}
: ${quasselcore_data:="/var/db/quasselcore"}
: ${quasselcore_log:="/var/log/quasselcore.log"}
: ${quasselcore_listen:="0.0.0.0,::"}
: ${quasselcore_args:=""}

pidfile="/var/run/${name}.pid"
procname="/usr/local/bin/quasselcore-rs"
command="/usr/sbin/daemon"
command_args="-f -p ${pidfile} -T ${name} -u ${quasselcore_user} -g ${quasselcore_group} ${procname} --configdir=${quasselcore_data} --listen=${quasselcore_listen} --logfile=${quasselcore_log} ${quasselcore_args}"

run_rc_command "$1"
```

## What is in this tree

| crate | what it is |
|---|---|
| `qs-datastream` | the `QDataStream`/`QVariant` codec the protocol uses |
| `qs-proto` | the handshake, the feature negotiation, the signal proxy layer |
| `qs-storage` | the SQLite and PostgreSQL backends |
| `qs-irc` | the IRC connections over TLS |
| `quasselcore-rs` | the core itself |

The design, and the plan of the rounds to come, are in
`../docs/rust/ARCHITECTURE.md`.

## Licence

Quassel is under the GNU General Public License, and so is this port: it is
a derivative work of the C++ tree, which stays in the repository next to it.
