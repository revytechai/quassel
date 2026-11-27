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

#include "testglobal.h"

#include <QLabel>
#include <QPushButton>
#include <QSignalSpy>

#include "client.h"
#include "buffermodel.h"
#include "qtui.h"
#include "integrationtestsupport.h"
#include "mainwin.h"
#include "multilineedit.h"
#include "network.h"
#include "networkmodel.h"
#include "standalonebufferwindow.h"

using namespace ::testing;

class StandaloneBufferWindowTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        test::QuasselTestSupport::ensureInitialized(Quassel::Monolithic);
    }
};

TEST_F(StandaloneBufferWindowTest, windowLifecycleTopicInputAndDeduplication)
{
    auto* qtUi = new QtUi();
    std::unique_ptr<AbstractUi> uiPtr(qtUi);
    Client client(std::move(uiPtr));
    qtUi->init();

    NetworkModel* model = Client::networkModel();
    ASSERT_NE(model, nullptr);

    NetworkId netId(1);
    BufferInfo chanBuf(BufferId(2), netId, BufferInfo::ChannelBuffer, 0, "#quassel");
    model->bufferUpdated(chanBuf);

    auto* net = new Network(netId, &client);
    net->setNetworkName("Freenode");
    Client::addNetwork(net);

    // 1. Direct window tests
    auto* window = new StandaloneBufferWindow(BufferId(2), nullptr);
    EXPECT_EQ(window->bufferId(), BufferId(2));
    EXPECT_EQ(window->bufferInfo().bufferName(), QStringLiteral("#quassel"));
    EXPECT_TRUE(window->windowTitle().contains(QStringLiteral("#quassel")));
    EXPECT_TRUE(window->windowTitle().contains(QStringLiteral("Freenode")));

    ASSERT_NE(window->topicLabel(), nullptr);
    ASSERT_NE(window->inputEdit(), nullptr);
    ASSERT_NE(window->sendButton(), nullptr);
    ASSERT_NE(window->toggleUsersButton(), nullptr);
    ASSERT_NE(window->nickView(), nullptr);

    // Test Users toggle button
    EXPECT_FALSE(window->toggleUsersButton()->isHidden());
    EXPECT_FALSE(window->areUsersVisible());
    EXPECT_FALSE(window->toggleUsersButton()->isChecked());

    window->setUsersVisible(true);
    EXPECT_TRUE(window->areUsersVisible());
    EXPECT_TRUE(window->toggleUsersButton()->isChecked());

    window->toggleUsers();
    EXPECT_FALSE(window->areUsersVisible());
    EXPECT_FALSE(window->toggleUsersButton()->isChecked());

    // Test sendInput
    window->inputEdit()->setText("Hello from standalone popped-out window");
    window->sendInput("Hello from standalone popped-out window");
    EXPECT_TRUE(window->inputEdit()->text().isEmpty());

    delete window;

    // 2. MainWin pop-out de-duplication tests
    MainWin* mainWin = QtUi::mainWindow();
    ASSERT_NE(mainWin, nullptr);

    BufferInfo generalBuf(BufferId(5), netId, BufferInfo::ChannelBuffer, 0, "#general");
    model->bufferUpdated(generalBuf);

    EXPECT_FALSE(mainWin->hasStandaloneWindow(BufferId(5)));

    // First pop-out creates window
    mainWin->popOutBuffer(BufferId(5));
    EXPECT_TRUE(mainWin->hasStandaloneWindow(BufferId(5)));
    EXPECT_TRUE(Client::bufferModel()->isBufferPoppedOut(BufferId(5)));
    StandaloneBufferWindow* first = mainWin->standaloneWindow(BufferId(5));
    ASSERT_NE(first, nullptr);

    // Second pop-out focuses existing window rather than creating a duplicate
    mainWin->popOutBuffer(BufferId(5));
    StandaloneBufferWindow* second = mainWin->standaloneWindow(BufferId(5));
    EXPECT_EQ(first, second);

    // Clean up
    first->close();
}
