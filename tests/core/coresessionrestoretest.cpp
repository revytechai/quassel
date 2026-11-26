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

#include <algorithm>
#include <memory>

#include <QCoreApplication>
#include <QDataStream>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QProcessEnvironment>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>

#include "core.h"
#include "coreidentity.h"
#include "corenetwork.h"
#include "coresession.h"
#include "network.h"
#include "quassel.h"
#include "sqlitestorage.h"
#include "types.h"

namespace {

// The name of the SQLite database file that the storage keeps in the configuration directory.
const QString storageDatabaseFileName = QStringLiteral("quassel-storage.sqlite");

// Redirects Quassel's configuration directory (and with it the SQLite database file) into the
// given sandbox directory, so that the test never touches the configuration of the user running
// it.
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

// The sandbox directory of the test run.  It is shared by all fixtures, because Quassel and
// the core are set up only once.
QTemporaryDir& sandboxDir()
{
    static QTemporaryDir sandbox;
    return sandbox;
}

// Processes the events until the given number of connections was accepted, or the timeout
// expires.
template<typename Counter>
bool waitForConnections(Counter&& connectionCount, int expected, int timeout = 5000)
{
    QElapsedTimer timer;
    timer.start();
    while (connectionCount() < expected && timer.elapsed() < timeout)
        QCoreApplication::processEvents(QEventLoop::AllEvents, 50);
    return connectionCount() >= expected;
}

// Processes the events for the given time, so that a connection that is still on its way can
// arrive before the test draws its conclusion.
void settle(int msecs = 500)
{
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < msecs)
        QCoreApplication::processEvents(QEventLoop::AllEvents, 50);
}

// Whether the network is flagged as connected in the database.  The flag says that the network
// should be connected, and the core uses it to reconnect the networks when it starts up again.
bool isNetworkConnected(UserId userId, NetworkId networkId)
{
    const std::vector<NetworkId> connected = Core::connectedNetworks(userId);
    return std::find(connected.begin(), connected.end(), networkId) != connected.end();
}

// The Core is a singleton, so it is created once and kept alive for the whole test run.
Core& coreInstance()
{
    static Core* core = []() {
        auto* core = new Core();
        return core;
    }();
    return *core;
}

// The core of the test run: the sandbox, the storage, the users, their networks and the core
// state that is restored.  All of it is set up once, because the Core is a singleton.
class TestCore
{
public:
    static TestCore& instance()
    {
        static TestCore core;
        return core;
    }

    //! Sets up the core and restores the sessions of the users whose state was persisted
    bool setUp()
    {
        if (_setUp)
            return true;

        // The sandbox must exist before Quassel is initialized: when it cannot be created, the
        // test has to abort, because Quassel would otherwise use the real configuration
        // directory of the user running the test.
        if (!redirectConfigToSandbox(sandboxDir()))
            return false;

        auto* quassel = new Quassel();
        Quassel::setupBuildInfo();
        // Monolithic run mode: the core does not open a listening port, so the test never
        // conflicts with a core that is already running on this machine.
        quassel->init(Quassel::Monolithic);

        // The core registers its storage backends when it is initialized, so it has to be
        // initialized before it can be set up.
        coreInstance().init();

        // The storage and its schema have to exist before the state can be restored.
        const QString setupError = Core::setup("restoretest", "restoretest", "SQLite", {}, "Database", {});
        if (!setupError.isEmpty()) {
            ADD_FAILURE() << "cannot set up the core: " << setupError.toUtf8().constData();
            return false;
        }

        // A storage instance of the test's own, to seed the core state and to read the
        // snapshots that the core persists.
        _storage = std::make_unique<SqliteStorage>();
        if (_storage->init({}, QProcessEnvironment::systemEnvironment(), false) != Storage::IsReady) {
            ADD_FAILURE() << "the storage of the test is not ready";
            return false;
        }

        // The two users whose sessions are restored.
        _firstUserId = Core::getUserId(QStringLiteral("restoretest"));
        _secondUserId = Core::addUser(QStringLiteral("restoretest2"), QStringLiteral("restoretest2"));
        if (!_firstUserId.isValid() || !_secondUserId.isValid()) {
            ADD_FAILURE() << "the users of the test could not be created";
            return false;
        }

        // A network of the first user that is flagged as connected, a network of the second
        // user that is flagged as connected, and a network of the first user that is not.
        IdentityId firstIdentity = createIdentity(_firstUserId, QStringLiteral("RestoreTest"));
        IdentityId secondIdentity = createIdentity(_secondUserId, QStringLiteral("RestoreTest2"));
        if (!firstIdentity.isValid() || !secondIdentity.isValid()) {
            ADD_FAILURE() << "the identities of the test could not be created";
            return false;
        }

        _firstNetworkId = createNetwork(_firstUserId, firstIdentity, QStringLiteral("ConnectedNet"), *_firstServer, true);
        _secondNetworkId = createNetwork(_secondUserId, secondIdentity, QStringLiteral("ConnectedNet2"), *_secondServer, true);
        _thirdNetworkId = createNetwork(_firstUserId, firstIdentity, QStringLiteral("DisconnectedNet"), *_thirdServer, false);
        if (!_firstNetworkId.isValid() || !_secondNetworkId.isValid() || !_thirdNetworkId.isValid()) {
            ADD_FAILURE() << "the networks of the test could not be created";
            return false;
        }

        // The core state that the core restores: the sessions of the two users.
        _storage->setCoreState(QVariantList{QVariant::fromValue(_firstUserId), QVariant::fromValue(_secondUserId)});
        // The storage of the test has done its work: only the storage of the core keeps the
        // database open, so that the two of them do not fight over the lock of the database.
        _storage.reset();

        // Every snapshot that the core persists from now on is logged, so that the test can
        // tell how many of them were written and what they hold.
        if (!installSnapshotLog()) {
            ADD_FAILURE() << "the snapshot log could not be installed";
            return false;
        }

        // The core restores the sessions of the two users.
        coreInstance().restoreState();

        _setUp = true;
        return true;
    }

    UserId firstUserId() const { return _firstUserId; }
    UserId secondUserId() const { return _secondUserId; }
    int firstServerConnections() const { return _firstServerConnections; }
    int secondServerConnections() const { return _secondServerConnections; }
    int thirdServerConnections() const { return _thirdServerConnections; }

private:
    TestCore()
        : _firstServer(std::make_unique<QTcpServer>())
        , _secondServer(std::make_unique<QTcpServer>())
        , _thirdServer(std::make_unique<QTcpServer>())
    {
        QObject::connect(_firstServer.get(), &QTcpServer::newConnection, _firstServer.get(), [this]() { ++_firstServerConnections; });
        QObject::connect(_secondServer.get(), &QTcpServer::newConnection, _secondServer.get(), [this]() { ++_secondServerConnections; });
        QObject::connect(_thirdServer.get(), &QTcpServer::newConnection, _thirdServer.get(), [this]() { ++_thirdServerConnections; });
    }

    IdentityId createIdentity(UserId userId, const QString& name)
    {
        CoreIdentity identity{IdentityId{1}};
        identity.setIdentityName(name);
        identity.setNicks(QStringList{name.toLower()});
        identity.setRealName(name);
        return Core::createIdentity(userId, identity);
    }

    //! Creates a network that points at the given local server
    NetworkId createNetwork(UserId userId, IdentityId identityId, const QString& networkName, QTcpServer& server, bool connected)
    {
        if (!server.listen(QHostAddress::LocalHost, 0))
            return NetworkId{};

        NetworkInfo info;
        info.networkName = networkName;
        info.identity = identityId;
        info.codecForEncoding = "UTF-8";
        info.codecForDecoding = "UTF-8";
        info.codecForServer = "UTF-8";

        Network::Server serverInfo;
        serverInfo.host = QStringLiteral("127.0.0.1");
        serverInfo.port = server.serverPort();
        info.serverList = {serverInfo};

        if (!Core::createNetwork(userId, info))
            return NetworkId{};

        if (connected)
            Core::setNetworkConnected(userId, info.networkId, true);

        return info.networkId;
    }

    //! Records every snapshot of the core state in a log table
    bool installSnapshotLog()
    {
        static int counter = 0;
        const QString connectionName = QStringLiteral("snapshotlog%1").arg(++counter);

        bool success = false;
        {
            QSqlDatabase db = QSqlDatabase::addDatabase("QSQLITE", connectionName);
            db.setDatabaseName(QDir(Quassel::configDirPath()).filePath(storageDatabaseFileName));
            if (db.open()) {
                QSqlQuery query(db);
                success = query.exec(QStringLiteral("CREATE TABLE IF NOT EXISTS core_state_log (id INTEGER PRIMARY KEY AUTOINCREMENT, value BLOB)"))
                    && query.exec(QStringLiteral("CREATE TRIGGER IF NOT EXISTS core_state_log_insert AFTER INSERT ON core_state "
                                                 "BEGIN INSERT INTO core_state_log (value) VALUES (NEW.value); END"))
                    && query.exec(QStringLiteral("CREATE TRIGGER IF NOT EXISTS core_state_log_update AFTER UPDATE ON core_state "
                                                 "BEGIN INSERT INTO core_state_log (value) VALUES (NEW.value); END"));
                if (!success)
                    ADD_FAILURE() << "cannot install the snapshot log: " << query.lastError().text().toUtf8().constData();
                db.close();
            }
            else {
                ADD_FAILURE() << "cannot open the storage database: " << db.lastError().text().toUtf8().constData();
            }
        }
        QSqlDatabase::removeDatabase(connectionName);

        return success;
    }

    bool _setUp{false};
    std::unique_ptr<SqliteStorage> _storage;
    UserId _firstUserId;
    UserId _secondUserId;
    NetworkId _firstNetworkId;
    NetworkId _secondNetworkId;
    NetworkId _thirdNetworkId;
    std::unique_ptr<QTcpServer> _firstServer;
    std::unique_ptr<QTcpServer> _secondServer;
    std::unique_ptr<QTcpServer> _thirdServer;
    int _firstServerConnections{0};
    int _secondServerConnections{0};
    int _thirdServerConnections{0};
};

// Reads the snapshots of the core state that the core persisted.
QList<QVariantList> persistedSnapshots()
{
    static int counter = 0;
    const QString connectionName = QStringLiteral("snapshotread%1").arg(++counter);

    QList<QVariantList> snapshots;
    {
        QSqlDatabase db = QSqlDatabase::addDatabase("QSQLITE", connectionName);
        db.setDatabaseName(QDir(Quassel::configDirPath()).filePath(storageDatabaseFileName));
        if (db.open()) {
            QSqlQuery query(db);
            if (query.exec(QStringLiteral("SELECT value FROM core_state_log ORDER BY id"))) {
                while (query.next()) {
                    QByteArray rawData = query.value(0).toByteArray();
                    QDataStream in(&rawData, QIODevice::ReadOnly);
                    in.setVersion(QDataStream::Qt_4_2);
                    QVariantList data;
                    in >> data;
                    snapshots << data;
                }
            }
            else {
                ADD_FAILURE() << "cannot read the snapshot log: " << query.lastError().text().toUtf8().constData();
            }
            db.close();
        }
        else {
            ADD_FAILURE() << "cannot open the storage database: " << db.lastError().text().toUtf8().constData();
        }
    }
    QSqlDatabase::removeDatabase(connectionName);

    return snapshots;
}

}  // namespace

class CoreSessionRestoreTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        // A sandbox that could not be created must abort the test: Quassel would otherwise use
        // the configuration directory of the user running the test.
        ASSERT_TRUE(redirectConfigToSandbox(sandboxDir())) << "the test sandbox directory could not be created";
        ASSERT_TRUE(TestCore::instance().setUp());
    }
};

// The sessions of the users whose state was persisted are restored, and every network that is
// flagged as connected is reconnected.  The state itself is persisted once, after all sessions
// have been restored: a state that is written while the restore is still running would hold
// only a part of the sessions.
TEST_F(CoreSessionRestoreTest, restoresEverySessionAndPersistsTheStateOnce)
{
    TestCore& core = TestCore::instance();

    // The networks of both users are reconnected: the session of both users was restored.
    ASSERT_TRUE(waitForConnections([&core]() { return core.firstServerConnections(); }, 1))
        << "the network of the first user must be reconnected";
    ASSERT_TRUE(waitForConnections([&core]() { return core.secondServerConnections(); }, 1))
        << "the network of the second user must be reconnected";

    // The network that is not flagged as connected stays disconnected.  The events are
    // processed for a while first, so that a connection that is still on its way can arrive.
    settle();
    EXPECT_EQ(0, core.thirdServerConnections())
        << "a network that is not flagged as connected must not be connected";

    // The core persisted the state once, and that snapshot holds both users.
    const QList<QVariantList> snapshots = persistedSnapshots();
    // No snapshot may hold only a part of the sessions: a core that stops while the restore is
    // still running would lose the sessions that were not restored yet.
    for (const QVariantList& snapshot : snapshots) {
        EXPECT_EQ(2, snapshot.size()) << "a snapshot must not hold only a part of the sessions";
    }
    ASSERT_EQ(1, snapshots.size()) << "the core must persist the core state only once";
    const QVariantList& snapshot = snapshots.front();
    ASSERT_EQ(2, snapshot.size()) << "the snapshot must hold the sessions of both users";
    // The order of the sessions is the order of the hash that holds them, so the snapshot is
    // compared as a set.
    EXPECT_TRUE(snapshot.contains(QVariant::fromValue(core.firstUserId())));
    EXPECT_TRUE(snapshot.contains(QVariant::fromValue(core.secondUserId())));
}

// The disconnected flag of a network is the flag that says that the network should be
// connected, and the core uses it to reconnect the networks when it starts up again.  Only a
// disconnect that the user asked for clears it.
class CoreNetworkDisconnectTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        // A sandbox that could not be created must abort the test: Quassel would otherwise use
        // the configuration directory of the user running the test.
        ASSERT_TRUE(redirectConfigToSandbox(sandboxDir())) << "the test sandbox directory could not be created";
        ASSERT_TRUE(TestCore::instance().setUp());
        // The sessions that the core restored are still busy in their own threads: let them
        // settle, so that they do not write to the database while the test writes to it.
        settle(500);
    }

    // Creates a user with a network that points at a local server, so that the test can tell
    // whether the core connected to it.
    struct LocalNetwork
    {
        UserId userId;
        NetworkId networkId;
        QTcpServer server;
        QTcpSocket* serverSocket{nullptr};
        int connections{0};
    };

    void createLocalNetwork(const QString& userName, LocalNetwork& local)
    {
        local.userId = Core::addUser(userName, userName);
        if (!local.userId.isValid())
            return;

        CoreIdentity identity{IdentityId{1}};
        identity.setIdentityName(userName);
        identity.setNicks(QStringList{userName.toLower()});
        identity.setRealName(userName);
        IdentityId identityId = Core::createIdentity(local.userId, identity);
        if (!identityId.isValid())
            return;

        if (!local.server.listen(QHostAddress::LocalHost, 0))
            return;

        QObject::connect(&local.server, &QTcpServer::newConnection, &local.server, [&local]() {
            ++local.connections;
            local.serverSocket = local.server.nextPendingConnection();
        });

        NetworkInfo info;
        info.networkName = userName;
        info.identity = identityId;
        info.codecForEncoding = "UTF-8";
        info.codecForDecoding = "UTF-8";
        info.codecForServer = "UTF-8";

        Network::Server serverInfo;
        serverInfo.host = QStringLiteral("127.0.0.1");
        serverInfo.port = local.server.serverPort();
        info.serverList = {serverInfo};

        if (!Core::createNetwork(local.userId, info))
            return;

        local.networkId = info.networkId;
        Core::setNetworkConnected(local.userId, local.networkId, true);
    }

    // Creates the user and its network, connects the network and waits until the core has
    // connected to the local server.  The network of the session is returned; a null pointer means
    // that the setup failed and that the test must not go on.
    CoreNetwork* connectLocalNetwork(const QString& userName, LocalNetwork& local)
    {
        createLocalNetwork(userName, local);
        if (!local.userId.isValid() || !local.networkId.isValid()) {
            ADD_FAILURE() << "the user and its network could not be created";
            return nullptr;
        }

        // The session connects the networks that are flagged as connected.
        _session = std::make_unique<CoreSession>(local.userId, false);
        CoreNetwork* network = _session->network(local.networkId);
        if (!network) {
            ADD_FAILURE() << "the session does not know the network of the user";
            return nullptr;
        }
        if (!waitForConnections([&local]() { return local.connections; }, 1)) {
            ADD_FAILURE() << "the network must be connected";
            return nullptr;
        }
        if (!local.serverSocket) {
            ADD_FAILURE() << "the local server did not accept the connection";
            return nullptr;
        }
        if (!isNetworkConnected(local.userId, local.networkId)) {
            ADD_FAILURE() << "the network must be flagged as connected";
            return nullptr;
        }
        return network;
    }

    // The session that holds the network of the test: the network belongs to the session, so the
    // session has to outlive it.
    std::unique_ptr<CoreSession> _session;
};

// A disconnect that the user asked for clears the connected flag of the network, also when the
// core shuts down before the socket callback arrives.
TEST_F(CoreNetworkDisconnectTest, userRequestedDisconnectClearsTheConnectedFlag)
{
    LocalNetwork local;
    CoreNetwork* network = connectLocalNetwork(QStringLiteral("disconnecttest"), local);
    ASSERT_NE(nullptr, network);

    // The user asks for the disconnect ...
    network->disconnectFromIrc(true, QStringLiteral("user request"), false);
    // ... and the core shuts down before the socket callback arrives.
    network->shutdown();

    // The server closes the connection, which makes the socket callback of the core run.
    local.serverSocket->disconnectFromHost();
    settle();

    // The user asked for the disconnect: the network must not be reconnected when the core is
    // started again.
    EXPECT_FALSE(isNetworkConnected(local.userId, local.networkId))
        << "a disconnect that the user asked for must clear the connected flag";
}

// A disconnect that the core asked for while shutting down keeps the connected flag of the
// network: the network has to be reconnected when the core is started again.
TEST_F(CoreNetworkDisconnectTest, shutdownKeepsTheConnectedFlag)
{
    LocalNetwork local;
    CoreNetwork* network = connectLocalNetwork(QStringLiteral("shutdowntest"), local);
    ASSERT_NE(nullptr, network);

    // The core shuts down.
    network->shutdown();

    // The server closes the connection, which makes the socket callback of the core run.
    local.serverSocket->disconnectFromHost();
    settle();

    // The core did not ask the user: the network keeps its connected flag.
    EXPECT_TRUE(isNetworkConnected(local.userId, local.networkId))
        << "a disconnect that the core asked for while shutting down must keep the connected flag";
}
