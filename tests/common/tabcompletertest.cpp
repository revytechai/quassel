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

#include <QLineEdit>

#include "client.h"
#include "ircchannel.h"
#include "ircuser.h"
#include "multilineedit.h"
#include "network.h"
#include "networkmodel.h"
#include "tabcompleter.h"
#include "integrationtestsupport.h"

using namespace ::testing;

class TabCompleterTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);
    }
};

TEST_F(TabCompleterTest, completesNickAndChannelPrefixScopedToExplicitBuffer)
{
    Client client(test::QuasselTestSupport::createTestUi());
    NetworkModel* netModel = Client::networkModel();
    ASSERT_NE(netModel, nullptr);

    NetworkId netId(1);
    BufferInfo chanBuf(BufferId(10), netId, BufferInfo::ChannelBuffer, 0, "#testchan");
    netModel->bufferUpdated(chanBuf);

    auto* net = new Network(netId, &client);
    net->setNetworkName("TestNet");
    Client::addNetwork(net);

    IrcChannel* chan = net->newIrcChannel(QStringLiteral("#testchan"));
    IrcUser* user = net->newIrcUser(QStringLiteral("revydev"));
    chan->joinIrcUser(user);

    // 1. Nick completion
    MultiLineEdit nickEdit;
    nickEdit.setText("rev");
    nickEdit.setCursorPosition(3);

    TabCompleter nickCompleter(&nickEdit, BufferId(10));
    nickCompleter.complete();

    EXPECT_EQ(nickEdit.text(), QStringLiteral("revydev: "));

    // 2. Channel prefix completion
    MultiLineEdit chanEdit;
    chanEdit.setText("#te");
    chanEdit.setCursorPosition(3);

    TabCompleter chanCompleter(&chanEdit, BufferId(10));
    chanCompleter.complete();

    EXPECT_EQ(chanEdit.text(), QStringLiteral("#testchan"));
}
