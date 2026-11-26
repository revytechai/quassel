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

#include <QSignalSpy>

#include "bufferviewfilter.h"
#include "client.h"
#include "clientbufferviewconfig.h"
#include "core.h"
#include "coreconnection.h"
#include "integrationtestsupport.h"
#include "network.h"
#include "networkmodel.h"
#include "protocol.h"

using namespace ::testing;

class TestBufferViewFilter : public BufferViewFilter
{
public:
    using BufferViewFilter::BufferViewFilter;
    using BufferViewFilter::filterAcceptsRow;
};

class ChannelSyncTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        test::QuasselTestSupport::ensureInitialized(Quassel::Monolithic);
    }
};

TEST_F(ChannelSyncTest, channelAndBufferViewSynchronization)
{
    Client client(test::QuasselTestSupport::createTestUi());
    Core core;
    core.init();

    test::InternalCoreConnectionBridge bridge(&core);

    ASSERT_TRUE(Client::coreConnection()->connectToCore());
    if (!Client::isConnected()) {
        QSignalSpy connectedSpy(&client, &Client::connected);
        ASSERT_TRUE(connectedSpy.wait(5000));
    }

    EXPECT_TRUE(Client::isConnected());
    EXPECT_EQ(CoreConnection::Synchronized, Client::coreConnection()->state());

    NetworkModel* model = Client::networkModel();
    ASSERT_NE(model, nullptr);

    NetworkId netId(1);
    BufferInfo statusBuf(BufferId(1), netId, BufferInfo::StatusBuffer, 0, "");
    BufferInfo channelQuassel(BufferId(2), netId, BufferInfo::ChannelBuffer, 0, "#quassel");
    BufferInfo channelDev(BufferId(3), netId, BufferInfo::ChannelBuffer, 0, "#dev");
    BufferInfo queryNick(BufferId(4), netId, BufferInfo::QueryBuffer, 0, "revhelix");

    // Feed buffer updates to the model
    model->bufferUpdated(statusBuf);
    model->bufferUpdated(channelQuassel);
    model->bufferUpdated(channelDev);
    model->bufferUpdated(queryNick);

    // Verify bufferId returns correct BufferId
    EXPECT_EQ(model->bufferId(netId, "#quassel"), BufferId(2));
    EXPECT_EQ(model->bufferId(netId, "#dev"), BufferId(3));
    EXPECT_EQ(model->bufferId(netId, "revhelix"), BufferId(4));
    EXPECT_FALSE(model->bufferId(netId, "#nonexistent").isValid());

    // Verify buffer item types and names
    EXPECT_EQ(model->bufferType(BufferId(2)), BufferInfo::ChannelBuffer);
    EXPECT_EQ(model->bufferName(BufferId(2)), QStringLiteral("#quassel"));

    EXPECT_EQ(model->bufferType(BufferId(3)), BufferInfo::ChannelBuffer);
    EXPECT_EQ(model->bufferName(BufferId(3)), QStringLiteral("#dev"));

    EXPECT_EQ(model->bufferType(BufferId(4)), BufferInfo::QueryBuffer);
    EXPECT_EQ(model->bufferName(BufferId(4)), QStringLiteral("revhelix"));

    // Verify BufferViewFilter integration
    ClientBufferViewConfig config(0);
    config.setNetworkId(netId);
    config.setAddNewBuffersAutomatically(true);
    config.setSortAlphabetically(true);
    config.addBuffer(BufferId(2), 0);

    TestBufferViewFilter filter(model, &config);

    // Model index for channel
    QModelIndex chanIndex = model->bufferIndex(BufferId(2));
    ASSERT_TRUE(chanIndex.isValid());

    // Verify filter accepts the row
    EXPECT_TRUE(filter.filterAcceptsRow(chanIndex.row(), chanIndex.parent()));

    // Test dynamic channel addition
    BufferInfo dynamicChan(BufferId(12), netId, BufferInfo::ChannelBuffer, 0, "#quassel-chat");
    model->bufferUpdated(dynamicChan);
    config.addBuffer(BufferId(12), 1);

    QModelIndex dynamicIndex = model->bufferIndex(BufferId(12));
    ASSERT_TRUE(dynamicIndex.isValid());
    EXPECT_TRUE(filter.filterAcceptsRow(dynamicIndex.row(), dynamicIndex.parent()));
    EXPECT_EQ(model->bufferId(netId, "#quassel-chat"), BufferId(12));
    EXPECT_EQ(model->bufferType(BufferId(12)), BufferInfo::ChannelBuffer);
    EXPECT_EQ(model->bufferName(BufferId(12)), QStringLiteral("#quassel-chat"));
}
