#include <gtest/gtest.h>

#include <QToolBar>
#include <QStandardItemModel>

#include "toolbaractionprovider.h"
#include "client.h"
#include "network.h"
#include "networkmodel.h"
#include "integrationtestsupport.h"

class ToolBarActionProviderTest : public ::testing::Test {
protected:
    static void SetUpTestSuite() {
        test::QuasselTestSupport::ensureInitialized(Quassel::Monolithic);
        s_client = new Client(test::QuasselTestSupport::createTestUi());
    }

    static void TearDownTestSuite() {
        delete s_client;
        s_client = nullptr;
    }

    void SetUp() override {
        provider = new ToolBarActionProvider();
    }

    void TearDown() override {
        delete provider;
        provider = nullptr;
    }

    void verifyNickActions(bool enabled) const {
        for (auto t : {
            NetworkModelController::NickQuery,
            NetworkModelController::NickWhois,
            NetworkModelController::NickOp,
            NetworkModelController::NickDeop,
            NetworkModelController::NickVoice,
            NetworkModelController::NickDevoice,
            NetworkModelController::NickKick,
            NetworkModelController::NickBan,
            NetworkModelController::NickKickBan,
        }) {
            EXPECT_EQ(provider->action(t)->isEnabled(), enabled);
        }
    }

    static inline Client* s_client{nullptr};
    ToolBarActionProvider* provider{nullptr};
};

TEST_F(ToolBarActionProviderTest, initialActionStatesAreDisabled)
{
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::JoinChannel)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::BufferPart)->isEnabled());
    verifyNickActions(false);
}

TEST_F(ToolBarActionProviderTest, addActionsToToolBars)
{
    QToolBar mainBar;
    provider->addActions(&mainBar, ToolBarActionProvider::MainToolBar);
    auto mainActions = mainBar.actions();
    ASSERT_EQ(mainActions.size(), 4);
    EXPECT_EQ(mainActions.at(0), provider->action(NetworkModelController::NetworkConnectAllWithDropdown));
    EXPECT_EQ(mainActions.at(1), provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown));
    EXPECT_EQ(mainActions.at(2), provider->action(NetworkModelController::JoinChannel));
    EXPECT_EQ(mainActions.at(3), provider->action(NetworkModelController::BufferPart));

    QToolBar nickBar;
    provider->addActions(&nickBar, ToolBarActionProvider::NickToolBar);
    auto nickActions = nickBar.actions();
    // 2 nick actions + 1 separator + 7 operator/moderator actions = 10
    ASSERT_EQ(nickActions.size(), 10);
    EXPECT_EQ(nickActions.at(0), provider->action(NetworkModelController::NickQuery));
    EXPECT_EQ(nickActions.at(1), provider->action(NetworkModelController::NickWhois));
    EXPECT_TRUE(nickActions.at(2)->isSeparator());
    EXPECT_EQ(nickActions.at(3), provider->action(NetworkModelController::NickOp));
}

TEST_F(ToolBarActionProviderTest, nickSelectionTogglesNickActions)
{
    QStandardItemModel model(2, 1);
    QModelIndexList selected;
    selected << model.index(0, 0);

    provider->onNickSelectionChanged(selected);
    verifyNickActions(true);

    provider->onNickSelectionChanged({});
    verifyNickActions(false);
}

TEST_F(ToolBarActionProviderTest, bufferPartTogglesWithActiveChannelBuffer)
{
    QStandardItemModel model(2, 1);
    QStandardItem* item = new QStandardItem("channel");
    item->setData(static_cast<int>(BufferInfo::ChannelBuffer), NetworkModel::BufferTypeRole);
    item->setData(true, NetworkModel::ItemActiveRole);
    model.setItem(0, 0, item);

    provider->onCurrentBufferChanged(model.index(0, 0));
    EXPECT_TRUE(provider->action(NetworkModelController::BufferPart)->isEnabled());

    item->setData(false, NetworkModel::ItemActiveRole);
    provider->onCurrentBufferChanged(model.index(0, 0));
    EXPECT_FALSE(provider->action(NetworkModelController::BufferPart)->isEnabled());

    item->setData(static_cast<int>(BufferInfo::QueryBuffer), NetworkModel::BufferTypeRole);
    item->setData(true, NetworkModel::ItemActiveRole);
    provider->onCurrentBufferChanged(model.index(0, 0));
    EXPECT_FALSE(provider->action(NetworkModelController::BufferPart)->isEnabled());

    provider->onCurrentBufferChanged({});
    EXPECT_FALSE(provider->action(NetworkModelController::BufferPart)->isEnabled());
}

TEST_F(ToolBarActionProviderTest, disconnectedFromCoreResetsAllState)
{
    QStandardItemModel model(1, 1);
    QModelIndexList selected;
    selected << model.index(0, 0);
    provider->onNickSelectionChanged(selected);
    EXPECT_TRUE(provider->action(NetworkModelController::NickOp)->isEnabled());

    provider->disconnectedFromCore();
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::JoinChannel)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::BufferPart)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NickOp)->isEnabled());
}

TEST_F(ToolBarActionProviderTest, networkConnectionStateTransitionsUpdateToolbarActions)
{
    NetworkId netId(42);
    QMetaObject::invokeMethod(Client::instance(), "coreNetworkCreated", Q_ARG(NetworkId, netId));

    Network* net = const_cast<Network*>(Client::network(netId));
    ASSERT_NE(net, nullptr);
    net->setNetworkName("Libera");
    net->setConnectionState(Network::Disconnected);

    // After network created in disconnected state, Connect is enabled, Disconnect is disabled
    EXPECT_TRUE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::JoinChannel)->isEnabled());

    // When connection state changes to Initialized (connected)
    net->setConnectionState(Network::Initialized);
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_TRUE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
    EXPECT_TRUE(provider->action(NetworkModelController::JoinChannel)->isEnabled());

    // When connection state transitions back to Disconnected
    net->setConnectionState(Network::Disconnected);
    EXPECT_TRUE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::JoinChannel)->isEnabled());

    // Clean up network
    QMetaObject::invokeMethod(Client::instance(), "coreNetworkRemoved", Q_ARG(NetworkId, netId));
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkConnectAllWithDropdown)->isEnabled());
    EXPECT_FALSE(provider->action(NetworkModelController::NetworkDisconnectAllWithDropdown)->isEnabled());
}
