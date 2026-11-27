/***************************************************************************
 *   Copyright (C) 2005-2026 by the Quassel Project                        *
 *   devel@quassel-irc.org                                                 *
 *                                                                         *
 *   This program is free software; you can redistribute it and/or modify  *
 *   it under the terms of the GNU General Public License as published by  *
 *   the Free Software Foundation; either version 2 of the License, or     *
 *   (at your option) version 3.                                           *
 *                                                                         *
 *   This program is distributed in the hope that it will be useful,       *
 *   but WITHOUT ANY WARRANTY; without even the implied warranty of        *
 *   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
 *   GNU General Public License for more details.                          *
 *                                                                         *
 *   You should have received a copy of the GNU General Public License     *
 *   along with this program; if not, write to the                         *
 *   Free Software Foundation, Inc.,                                       *
 *   51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.         *
 ***************************************************************************/

#include <QFile>
#include <QSettings>
#include <QTemporaryDir>

#include "clientsettings.h"
#include "coreaccount.h"
#include "integrationtestsupport.h"
#include "testglobal.h"

using namespace ::testing;

// Tests for CoreAccount getters, setters, serialization, and equality

TEST(CoreAccountTest, accountIdAndValidity)
{
    CoreAccount emptyAccount;
    EXPECT_EQ(emptyAccount.accountId(), AccountId(0));
    EXPECT_FALSE(emptyAccount.isValid());

    emptyAccount.setAccountId(AccountId(42));
    EXPECT_EQ(emptyAccount.accountId(), AccountId(42));
    EXPECT_TRUE(emptyAccount.isValid());

    CoreAccount initializedAccount(AccountId(100));
    EXPECT_EQ(initializedAccount.accountId(), AccountId(100));
    EXPECT_TRUE(initializedAccount.isValid());
}

TEST(CoreAccountTest, accountNameAndInternal)
{
    CoreAccount account;
    account.setAccountName("My IRC Server");
    EXPECT_EQ(account.accountName(), "My IRC Server");
    EXPECT_FALSE(account.isInternal());

    account.setInternal(true);
    EXPECT_TRUE(account.isInternal());
    EXPECT_EQ(account.accountName(), CoreAccount::tr("Internal Core"));

    account.setInternal(false);
    EXPECT_FALSE(account.isInternal());
    EXPECT_EQ(account.accountName(), "My IRC Server");
}

TEST(CoreAccountTest, uuidGetterSetter)
{
    CoreAccount account;
    EXPECT_TRUE(account.uuid().isNull());

    QUuid testUuid = QUuid::createUuid();
    account.setUuid(testUuid);
    EXPECT_EQ(account.uuid(), testUuid);
}

TEST(CoreAccountTest, userPasswordAndStorePassword)
{
    CoreAccount account;
    EXPECT_TRUE(account.user().isEmpty());
    EXPECT_TRUE(account.password().isEmpty());
    EXPECT_FALSE(account.storePassword());

    account.setUser("revhelix");
    EXPECT_EQ(account.user(), "revhelix");

    account.setPassword("SecretPassword123!");
    EXPECT_EQ(account.password(), "SecretPassword123!");

    account.setStorePassword(true);
    EXPECT_TRUE(account.storePassword());

    account.setStorePassword(false);
    EXPECT_FALSE(account.storePassword());
}

TEST(CoreAccountTest, hostNameAndPort)
{
    CoreAccount account;
    EXPECT_TRUE(account.hostName().isEmpty());
    EXPECT_EQ(account.port(), 4242u);

    account.setHostName("irc.revynet.org");
    EXPECT_EQ(account.hostName(), "irc.revynet.org");

    account.setPort(6667);
    EXPECT_EQ(account.port(), 6667u);
}

TEST(CoreAccountTest, proxySettings)
{
    CoreAccount account;
    EXPECT_EQ(account.proxyType(), QNetworkProxy::DefaultProxy);
    EXPECT_TRUE(account.proxyUser().isEmpty());
    EXPECT_TRUE(account.proxyPassword().isEmpty());
    EXPECT_TRUE(account.proxyHostName().isEmpty());
    EXPECT_EQ(account.proxyPort(), 8080u);

    account.setProxyType(QNetworkProxy::Socks5Proxy);
    EXPECT_EQ(account.proxyType(), QNetworkProxy::Socks5Proxy);

    account.setProxyUser("proxyUser");
    EXPECT_EQ(account.proxyUser(), "proxyUser");

    account.setProxyPassword("proxyPass");
    EXPECT_EQ(account.proxyPassword(), "proxyPass");

    account.setProxyHostName("127.0.0.1");
    EXPECT_EQ(account.proxyHostName(), "127.0.0.1");

    account.setProxyPort(1080);
    EXPECT_EQ(account.proxyPort(), 1080u);
}

TEST(CoreAccountTest, variantMapRoundTrip)
{
    CoreAccount original(AccountId(5));
    QUuid uuid = QUuid::createUuid();
    original.setUuid(uuid);
    original.setAccountName("Production Core");
    original.setHostName("core.example.org");
    original.setPort(4242);
    original.setUser("alice");
    original.setPassword("s3cr3t");
    original.setStorePassword(true);
    original.setProxyType(QNetworkProxy::Socks5Proxy);
    original.setProxyHostName("proxy.example.org");
    original.setProxyPort(9050);
    original.setProxyUser("puser");
    original.setProxyPassword("ppass");

    // Without forcePassword: password is included if storePassword is true
    QVariantMap map = original.toVariantMap(false);
    EXPECT_EQ(map.value("AccountId").toInt(), 5);
    EXPECT_EQ(map.value("AccountName").toString(), "Production Core");
    EXPECT_EQ(map.value("HostName").toString(), "core.example.org");
    EXPECT_EQ(map.value("Port").toUInt(), 4242u);
    EXPECT_EQ(map.value("User").toString(), "alice");
    EXPECT_EQ(map.value("Password").toString(), "s3cr3t");
    EXPECT_TRUE(map.value("StorePassword").toBool());

    // Deserialize into fresh account
    CoreAccount restored;
    restored.fromVariantMap(map);
    EXPECT_EQ(restored.accountId(), AccountId(5));
    EXPECT_EQ(restored.uuid(), uuid);
    EXPECT_EQ(restored.accountName(), "Production Core");
    EXPECT_EQ(restored.hostName(), "core.example.org");
    EXPECT_EQ(restored.port(), 4242u);
    EXPECT_EQ(restored.user(), "alice");
    EXPECT_EQ(restored.password(), "s3cr3t");
    EXPECT_TRUE(restored.storePassword());
    EXPECT_EQ(restored.proxyType(), QNetworkProxy::Socks5Proxy);
    EXPECT_EQ(restored.proxyHostName(), "proxy.example.org");
    EXPECT_EQ(restored.proxyPort(), 9050u);
    EXPECT_EQ(restored.proxyUser(), "puser");
    EXPECT_EQ(restored.proxyPassword(), "ppass");

    // Equality check
    EXPECT_EQ(original, restored);

    // Operator !=
    restored.setPort(4243);
    EXPECT_NE(original, restored);
}

// Tests for CoreAccountSettings

TEST(CoreAccountTest, settingsAccountManagement)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    CoreAccountSettings settings;
    settings.clearAccounts();
    EXPECT_TRUE(settings.knownAccounts().isEmpty());

    // Store account 1
    QVariantMap data1;
    data1["AccountId"] = 1;
    data1["AccountName"] = "Server One";
    data1["HostName"] = "irc1.example.org";
    data1["Port"] = 4242;
    data1["User"] = "user1";
    settings.storeAccountData(AccountId(1), data1);

    // Store account 2
    QVariantMap data2;
    data2["AccountId"] = 2;
    data2["AccountName"] = "Server Two";
    data2["HostName"] = "irc2.example.org";
    data2["Port"] = 6697;
    data2["User"] = "user2";
    settings.storeAccountData(AccountId(2), data2);

    auto accounts = settings.knownAccounts();
    EXPECT_EQ(accounts.size(), 2);
    EXPECT_TRUE(accounts.contains(AccountId(1)));
    EXPECT_TRUE(accounts.contains(AccountId(2)));

    // Retrieve data
    QVariantMap retrieved1 = settings.retrieveAccountData(AccountId(1));
    EXPECT_EQ(retrieved1.value("AccountName").toString(), "Server One");
    EXPECT_EQ(retrieved1.value("HostName").toString(), "irc1.example.org");

    // Remove account 1
    settings.removeAccount(AccountId(1));
    accounts = settings.knownAccounts();
    EXPECT_EQ(accounts.size(), 1);
    EXPECT_FALSE(accounts.contains(AccountId(1)));
    EXPECT_TRUE(accounts.contains(AccountId(2)));

    // Clear all
    settings.clearAccounts();
    EXPECT_TRUE(settings.knownAccounts().isEmpty());
}

TEST(CoreAccountTest, settingsAutoConnectOptions)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    CoreAccountSettings settings;

    // LastAccount
    settings.setLastAccount(AccountId(7));
    EXPECT_EQ(settings.lastAccount(), AccountId(7));

    // AutoConnectAccount
    settings.setAutoConnectAccount(AccountId(2));
    EXPECT_EQ(settings.autoConnectAccount(), AccountId(2));

    // AutoConnectOnStartup
    settings.setAutoConnectOnStartup(true);
    EXPECT_TRUE(settings.autoConnectOnStartup());
    settings.setAutoConnectOnStartup(false);
    EXPECT_FALSE(settings.autoConnectOnStartup());

    // AutoConnectToFixedAccount
    settings.setAutoConnectToFixedAccount(true);
    EXPECT_TRUE(settings.autoConnectToFixedAccount());
    settings.setAutoConnectToFixedAccount(false);
    EXPECT_FALSE(settings.autoConnectToFixedAccount());
}

TEST(CoreAccountTest, flatpakMigration)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    QTemporaryDir tempDir;
    ASSERT_TRUE(tempDir.isValid());
    QString flatpakConf = tempDir.path() + "/quasselclient.conf";

    // Create a mock Flatpak config file
    {
        QSettings s(flatpakConf, QSettings::IniFormat);
        s.beginGroup("CoreAccounts");
        s.setValue("AutoConnectAccount", 2);
        s.setValue("AutoConnectOnStartup", true);
        s.setValue("AutoConnectToFixedAccount", true);
        s.setValue("LastAccount", 2);

        s.beginGroup("2");
        s.setValue("AccountId", 2);
        s.setValue("AccountName", "revynet");
        s.setValue("HostName", "irc.revynet.org");
        s.setValue("Port", 4242);
        s.setValue("User", "revhelix");
        s.setValue("Password", "topsecret");
        s.setValue("StorePassword", true);
        s.endGroup();

        s.endGroup();
    }

    CoreAccountSettings settings;
    settings.clearAccounts();
    EXPECT_TRUE(settings.knownAccounts().isEmpty());

    // Run migration from the mock path
    bool migrated = settings.checkAndMigrateFlatpakConfig(flatpakConf);
    EXPECT_TRUE(migrated);

    // Verify imported state
    EXPECT_EQ(settings.autoConnectAccount(), AccountId(2));
    EXPECT_TRUE(settings.autoConnectOnStartup());
    EXPECT_TRUE(settings.autoConnectToFixedAccount());
    EXPECT_EQ(settings.lastAccount(), AccountId(2));

    auto accounts = settings.knownAccounts();
    EXPECT_TRUE(accounts.contains(AccountId(2)));

    QVariantMap data = settings.retrieveAccountData(AccountId(2));
    EXPECT_EQ(data.value("AccountName").toString(), "revynet");
    EXPECT_EQ(data.value("HostName").toString(), "irc.revynet.org");
    EXPECT_EQ(data.value("User").toString(), "revhelix");

    // Clean up
    settings.clearAccounts();
}

TEST(CoreAccountTest, debugStreamRedactsPasswords)
{
    CoreAccount acc(AccountId(1));
    acc.setAccountName("TestAccount");
    acc.setUser("alice");
    acc.setPassword("supersecretpassword123");
    acc.setProxyPassword("proxysecret456");

    QString str;
    QDebug dbg(&str);
    dbg << acc;

    EXPECT_FALSE(str.contains("supersecretpassword123"));
    EXPECT_FALSE(str.contains("proxysecret456"));
    EXPECT_TRUE(str.contains("<redacted>"));
}
