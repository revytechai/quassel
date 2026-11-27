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

#include "buffermodel.h"
#include "client.h"
#include "networkmodel.h"
#include "integrationtestsupport.h"

using namespace ::testing;

class BufferModelTest : public ::testing::Test
{
protected:
    void SetUp() override
    {
        test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);
    }
};

TEST_F(BufferModelTest, popOutBufferEmitsSignal)
{
    Client client(test::QuasselTestSupport::createTestUi());
    auto* model = Client::bufferModel();
    ASSERT_NE(model, nullptr);

    QSignalSpy spy(model, &BufferModel::bufferPopOutRequested);
    ASSERT_TRUE(spy.isValid());

    BufferId bufId(42);
    model->popOutBuffer(bufId);

    ASSERT_EQ(spy.count(), 1);
    EXPECT_EQ(spy.takeFirst().at(0).value<BufferId>(), bufId);
}
