/***************************************************************************
 *   Copyright (C) 2005-2026 by the Quassel Project                        *
 *   devel@quassel-irc.org                                                 *
 *                                                                         *
 *   This program is free software; you can redistribute it and/or modify  *
 *   it under the terms of the GNU General Public License as published by  *
 *   the Free Software Foundation; either version 2 of the License, or     *
 *   (at your option) any later version.                                   *
 *                                                                         *
 *   This program is distributed in the hope that it will be useful,       *
 *   but WITHOUT ANY WARRANTY; without even the implied warranty of        *
 *   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
 *   GNU General Public License for more details.                          *
 *                                                                         *
 *   You should have received a copy of the GNU General Public License     *
 *   along with this program; if not, write to the Free Software         *
 *   Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.
 ***************************************************************************/

#include "testglobal.h"

#include <memory>

#include <QDir>
#include <QProcessEnvironment>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTemporaryDir>

#include "network.h"
#include "postgresqlstorage.h"
#include "quassel.h"
#include "sqlitestorage.h"
#include "types.h"

namespace {

// The path a network takes to the storage: a client either creates a network, or it changes a
// network it created before.
enum class NetworkWritePath
{
    Create,
    Update
};

// The name of the SQLite database file that the storage keeps in the configuration directory.
const QString storageDatabaseFileName = QStringLiteral("quassel-storage.sqlite");

// Creates the sandbox directory for the test run and redirects Quassel's configuration
// directory (and with it the SQLite database file) into it, so that the test never touches the
// configuration of the user running it.
//
// Returns false when the sandbox directory could not be created.  Nothing is changed in that
// case: Quassel would otherwise fall back to the configuration directory of the user running
// the test, and the test would write to the database of that user.
bool redirectConfigToSandbox(const QTemporaryDir& sandbox)
{
    if (!sandbox.isValid())
        return false;

    QByteArray tempPath = sandbox.path().toUtf8();
    qputenv("HOME", tempPath);
    qputenv("XDG_CONFIG_HOME", tempPath);
    qputenv("XDG_DATA_HOME", tempPath);
    qputenv("XDG_CACHE_HOME", tempPath);
    qputenv("XDG_STATE_HOME", tempPath);
    return true;
}

// The codec columns of the network table are declared NOT NULL, so a null codec must never be
// bound as SQL NULL.  Both SQL backends bind them through the same helper.
//
// There is no PostgreSQL server and no PostgreSQL driver on the test machine, so the test
// checks the binding of the PostgreSQL backend itself: the values that
// PostgreSqlStorage::bindNetworkInfo() binds are stored in a table with the same NOT NULL
// constraints as the PostgreSQL schema.
class PostgreSqlBindingProbe : public PostgreSqlStorage
{
public:
    //! Binds the network info the way the PostgreSQL backend does
    void bindNetworkInfo(QSqlQuery& query, const NetworkInfo& info)
    {
        PostgreSqlStorage::bindNetworkInfo(query, info);
    }

    //! The shared helper that turns a null codec into a value that can be bound
    static QString bindCodec(const QByteArray& codec) { return bindableCodec(codec); }
};

// A temporary SQLite database with the codec columns of the network table, to find out what a
// backend binds for a null codec.
class CodecBindingDatabase
{
public:
    CodecBindingDatabase()
    {
        static int counter = 0;
        _connectionName = QStringLiteral("codecbinding%1").arg(++counter);
        _db = QSqlDatabase::addDatabase("QSQLITE", _connectionName);
        _db.setDatabaseName(QStringLiteral(":memory:"));
        _isOpen = _db.open();
    }

    ~CodecBindingDatabase()
    {
        _db.close();
        _db = QSqlDatabase();
        QSqlDatabase::removeDatabase(_connectionName);
    }

    bool isOpen() const { return _isOpen; }
    QSqlDatabase& database() { return _db; }

private:
    QString _connectionName;
    QSqlDatabase _db;
    bool _isOpen{false};
};

}  // namespace

class SqliteStorageTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        // The sandbox directory must exist before Quassel is initialized: when it cannot be
        // created, the test has to abort, because Quassel would otherwise use the real
        // configuration directory of the user running the test.
        static QTemporaryDir sandbox;
        ASSERT_TRUE(redirectConfigToSandbox(sandbox)) << "the test sandbox directory could not be created";

        static bool initialized = false;
        if (!initialized) {
            auto* quassel = new Quassel();
            Quassel::setupBuildInfo();
            quassel->init(Quassel::CoreOnly);
            initialized = true;
        }

        // The storage must live in the sandbox, and nowhere else.
        ASSERT_TRUE(QDir(Quassel::configDirPath()).absolutePath().startsWith(sandbox.path()))
            << "the test must not use the configuration directory of the user running it";

        _storage = std::make_unique<SqliteStorage>();
        if (_storage->init({}, QProcessEnvironment::systemEnvironment(), false) == Storage::NeedsSetup) {
            ASSERT_TRUE(_storage->setup({}, QProcessEnvironment::systemEnvironment(), false));
        }
        ASSERT_EQ(Storage::IsReady, _storage->init({}, QProcessEnvironment::systemEnvironment(), false));
    }

    UserId addUser(const QString& name)
    {
        UserId userId = _storage->addUser(name, "testpassword");
        EXPECT_TRUE(userId.isValid());
        return userId;
    }

    // Reads the raw codec columns of the given network, to tell an empty string apart from NULL
    QStringList rawCodecs(NetworkId networkId)
    {
        static int counter = 0;
        const QString connectionName = QStringLiteral("codeccheck%1").arg(++counter);

        QStringList result;
        {
            QSqlDatabase db = QSqlDatabase::addDatabase("QSQLITE", connectionName);
            // The storage keeps its database in the configuration directory, so the path has to
            // be built with a path join: concatenating the strings would open another file.
            db.setDatabaseName(QDir(Quassel::configDirPath()).filePath(storageDatabaseFileName));
            if (db.open()) {
                QSqlQuery query(db);
                if (query.exec(QStringLiteral("SELECT typeof(encodingcodec), encodingcodec, typeof(decodingcodec), decodingcodec, "
                                               "typeof(servercodec), servercodec FROM network WHERE networkid = %1")
                                        .arg(networkId.toInt()))
                    && query.first()) {
                    for (int i = 0; i < 6; ++i)
                        result << query.value(i).toString();
                }
                else {
                    ADD_FAILURE() << "cannot read the network row: " << query.lastError().text().toUtf8().constData();
                }
            }
            else {
                ADD_FAILURE() << "cannot open the storage database: " << db.lastError().text().toUtf8().constData();
            }
            db.close();
        }
        // The connection has to be removed explicitly: destroying the QSqlDatabase object does
        // not remove the connection that was registered with addDatabase().
        QSqlDatabase::removeDatabase(connectionName);

        return result;
    }

    // Looks up the stored information of the given network.  The storage returns the networks
    // of a user, so the network has to be picked from that list.
    NetworkInfo networkInfo(UserId userId, NetworkId networkId)
    {
        for (const NetworkInfo& info : _storage->networks(userId)) {
            if (info.networkId == networkId)
                return info;
        }

        ADD_FAILURE() << "the network was not found in the storage";
        return NetworkInfo{};
    }

    // Sets up a network for the given user, stores it and reads it back, checking that the
    // codec columns are stored as empty strings rather than as SQL NULL.  On the create path
    // the network is stored with the codecs of the given info; on the update path it is stored
    // first with those codecs and then updated with null codecs, which is what a client
    // without custom encodings sends when it changes a network.
    void expectNetworkWithNullCodecs(UserId userId, NetworkInfo info, NetworkWritePath path)
    {
        NetworkId networkId = _storage->createNetwork(userId, info);
        ASSERT_TRUE(networkId.isValid()) << "creating a network must not fail";

        if (path == NetworkWritePath::Update) {
            info.networkId = networkId;
            // A client without custom encodings sends null codecs.  They have to be assigned
            // explicitly: clearing a byte array leaves an empty, but not null one, which is a
            // different value than the one the client sends.
            info.codecForEncoding = QByteArray();
            info.codecForDecoding = QByteArray();
            info.codecForServer = QByteArray();
            ASSERT_TRUE(info.codecForEncoding.isNull()) << "the encoding codec must be null";
            ASSERT_TRUE(info.codecForDecoding.isNull()) << "the decoding codec must be null";
            ASSERT_TRUE(info.codecForServer.isNull()) << "the server codec must be null";

            EXPECT_TRUE(_storage->updateNetwork(userId, info)) << "updating a network with null codecs must not fail";
        }

        ASSERT_EQ(1u, _storage->networks(userId).size());
        NetworkInfo stored = networkInfo(userId, networkId);
        EXPECT_TRUE(stored.codecForEncoding.isEmpty());
        EXPECT_TRUE(stored.codecForDecoding.isEmpty());
        EXPECT_TRUE(stored.codecForServer.isEmpty());

        // The codecs are stored as empty strings, not as SQL NULL
        QStringList codecs = rawCodecs(networkId);
        ASSERT_EQ(6, codecs.size());
        EXPECT_EQ("text", codecs[0]) << "encodingcodec must not be NULL";
        EXPECT_EQ("", codecs[1]);
        EXPECT_EQ("text", codecs[2]) << "decodingcodec must not be NULL";
        EXPECT_EQ("", codecs[3]);
        EXPECT_EQ("text", codecs[4]) << "servercodec must not be NULL";
        EXPECT_EQ("", codecs[5]);
    }

    std::unique_ptr<SqliteStorage> _storage;
};

// A client that does not use custom encodings sends null codecs.  The codec columns are
// declared NOT NULL, so the core must not bind those nulls as SQL NULL.
TEST_F(SqliteStorageTest, createsNetworkWithNullCodecs)
{
    UserId userId = addUser("null-codecs");
    ASSERT_TRUE(userId.isValid());

    NetworkInfo info;
    info.networkName = QStringLiteral("NullCodecs");
    ASSERT_TRUE(info.codecForEncoding.isNull());
    ASSERT_TRUE(info.codecForDecoding.isNull());
    ASSERT_TRUE(info.codecForServer.isNull());

    expectNetworkWithNullCodecs(userId, info, NetworkWritePath::Create);
}

// The same holds for the update path, which the client uses when it changes a network.
TEST_F(SqliteStorageTest, updatesNetworkWithNullCodecs)
{
    UserId userId = addUser("null-codec-update");
    ASSERT_TRUE(userId.isValid());

    NetworkInfo info;
    info.networkName = QStringLiteral("CodecUpdate");
    info.codecForEncoding = "UTF-8";
    info.codecForDecoding = "UTF-8";
    info.codecForServer = "UTF-8";

    expectNetworkWithNullCodecs(userId, info, NetworkWritePath::Update);
}

// A client that does use custom encodings keeps its codecs.
TEST_F(SqliteStorageTest, createsNetworkWithCustomCodecs)
{
    UserId userId = addUser("custom-codecs");
    ASSERT_TRUE(userId.isValid());

    NetworkInfo info;
    info.networkName = QStringLiteral("CustomCodecs");
    info.codecForEncoding = "UTF-8";
    info.codecForDecoding = "ISO-8859-15";
    info.codecForServer = "UTF-8";

    NetworkId networkId = _storage->createNetwork(userId, info);
    ASSERT_TRUE(networkId.isValid());

    ASSERT_EQ(1u, _storage->networks(userId).size());
    NetworkInfo stored = networkInfo(userId, networkId);
    EXPECT_EQ("UTF-8", stored.codecForEncoding);
    EXPECT_EQ("ISO-8859-15", stored.codecForDecoding);
    EXPECT_EQ("UTF-8", stored.codecForServer);

    QStringList codecs = rawCodecs(networkId);
    ASSERT_EQ(6, codecs.size());
    EXPECT_EQ("UTF-8", codecs[1]);
    EXPECT_EQ("ISO-8859-15", codecs[3]);
    EXPECT_EQ("UTF-8", codecs[5]);
}

// Reading the raw codec columns must not leave a named SQL connection behind: the connections
// accumulate with every call, and they keep the temporary database open.
TEST_F(SqliteStorageTest, doesNotLeakSqlConnections)
{
    UserId userId = addUser("connection-leak");
    ASSERT_TRUE(userId.isValid());

    NetworkInfo info;
    info.networkName = QStringLiteral("ConnectionLeak");
    NetworkId networkId = _storage->createNetwork(userId, info);
    ASSERT_TRUE(networkId.isValid());

    const QStringList connectionsBefore = QSqlDatabase::connectionNames();
    ASSERT_EQ(6, rawCodecs(networkId).size());
    EXPECT_EQ(connectionsBefore, QSqlDatabase::connectionNames())
        << "reading the raw codec columns must not leave a SQL connection behind";
}

// The PostgreSQL backend must bind the codec columns the same way as the SQLite backend: a
// null codec must not be bound as SQL NULL, because the codec columns are NOT NULL.
//
// There is no PostgreSQL server and no PostgreSQL driver on the test machine, so this is a
// binding-level test: it binds the values the way PostgreSqlStorage::bindNetworkInfo() does
// and stores them in a table with the same NOT NULL constraints.  Reverting the PostgreSQL
// binding to the plain conversion binds SQL NULL, and the insert fails.
TEST_F(SqliteStorageTest, postgresBindingDoesNotBindNullCodecs)
{
    // The shared helper turns a null codec into an empty, but not null string.
    EXPECT_FALSE(PostgreSqlBindingProbe::bindCodec(QByteArray()).isNull())
        << "a null codec must not be bound as SQL NULL";
    EXPECT_EQ(QString(""), PostgreSqlBindingProbe::bindCodec(QByteArray()));
    EXPECT_EQ(QString("UTF-8"), PostgreSqlBindingProbe::bindCodec(QByteArray("UTF-8")));

    CodecBindingDatabase database;
    ASSERT_TRUE(database.isOpen());

    QSqlQuery create(database.database());
    ASSERT_TRUE(create.exec(QStringLiteral("CREATE TABLE network ("
                                           "networkid INTEGER PRIMARY KEY, "
                                           "encodingcodec TEXT NOT NULL, "
                                           "decodingcodec TEXT NOT NULL, "
                                           "servercodec TEXT NOT NULL)")));

    NetworkInfo info;
    info.networkName = QStringLiteral("PostgresCodecs");
    ASSERT_TRUE(info.codecForEncoding.isNull());
    ASSERT_TRUE(info.codecForDecoding.isNull());
    ASSERT_TRUE(info.codecForServer.isNull());

    QSqlQuery insert(database.database());
    insert.prepare(QStringLiteral("INSERT INTO network (networkid, encodingcodec, decodingcodec, servercodec) "
                                   "VALUES (1, :encodingcodec, :decodingcodec, :servercodec)"));

    PostgreSqlBindingProbe probe;
    probe.bindNetworkInfo(insert, info);

    ASSERT_TRUE(insert.exec()) << "the PostgreSQL binding must not bind SQL NULL: "
                                << insert.lastError().text().toUtf8().constData();

    QSqlQuery select(database.database());
    ASSERT_TRUE(select.exec(QStringLiteral("SELECT typeof(encodingcodec), encodingcodec, typeof(decodingcodec), decodingcodec, "
                                           "typeof(servercodec), servercodec FROM network")));
    ASSERT_TRUE(select.first());
    EXPECT_EQ("text", select.value(0).toString()) << "encodingcodec must not be NULL";
    EXPECT_EQ("", select.value(1).toString());
    EXPECT_EQ("text", select.value(2).toString()) << "decodingcodec must not be NULL";
    EXPECT_EQ("", select.value(3).toString());
    EXPECT_EQ("text", select.value(4).toString()) << "servercodec must not be NULL";
    EXPECT_EQ("", select.value(5).toString());
}

// The test must not use the configuration of the user running it when the sandbox directory
// cannot be created: Quassel would fall back to the configuration directory of that user.
TEST(SqliteStorageSandboxTest, refusesToRedirectWithoutASandbox)
{
    // A sandbox directory whose parent does not exist: it cannot be created.
    QTemporaryDir broken{QStringLiteral("/nonexistent-quassel-sandbox/quassel-test/")};
    ASSERT_FALSE(broken.isValid()) << "the broken sandbox must not be valid";

    const QByteArray configHomeBefore = qgetenv("XDG_CONFIG_HOME");
    const QByteArray homeBefore = qgetenv("HOME");

    EXPECT_FALSE(redirectConfigToSandbox(broken));

    EXPECT_EQ(configHomeBefore, qgetenv("XDG_CONFIG_HOME"))
        << "the configuration must not be redirected without a sandbox";
    EXPECT_EQ(homeBefore, qgetenv("HOME")) << "the home directory must not be redirected without a sandbox";
}
