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

#include <cstdint>

#include <QByteArray>
#include <QDataStream>
#include <QObject>
#include <QVariant>

#include "bufferinfo.h"
#include "quassel.h"
#include "testglobal.h"
#include "types.h"

using namespace ::testing;

class EnumHolder
{
    Q_GADGET

public:
    enum class Enum16 : uint16_t {};
    enum class Enum32 : uint32_t {};

    enum class EnumQt16 : uint16_t {};
    Q_ENUM(EnumQt16)
    enum class EnumQt32 : uint32_t {};
    Q_ENUM(EnumQt32)
};

// Verify that enums are (de)serialized as their underlying type
TEST(TypesTest, enumSerialization)
{
    QByteArray data;
    QDataStream out(&data, QIODevice::WriteOnly);

    // Serialize
    out << EnumHolder::Enum16(0xabcd);
    ASSERT_THAT(data.size(), Eq(2));
    out << EnumHolder::Enum32(0x123456);
    ASSERT_THAT(data.size(), Eq(6));
    out << EnumHolder::EnumQt16(0x4321);
    ASSERT_THAT(data.size(), Eq(8));
    out << EnumHolder::Enum32(0xfedcba);
    ASSERT_THAT(data.size(), Eq(12));
    ASSERT_THAT(out.status(), Eq(QDataStream::Status::Ok));

    // Deserialize
    QDataStream in(data);
    EnumHolder::Enum16 enum16;
    EnumHolder::Enum32 enum32;
    EnumHolder::EnumQt16 enumQt16;
    EnumHolder::EnumQt32 enumQt32;
    in >> enum16  >> enum32 >> enumQt16 >> enumQt32;
    ASSERT_THAT(in.status(), Eq(QDataStream::Status::Ok));
    EXPECT_TRUE(in.atEnd());

    EXPECT_THAT((int)enum16, Eq(0xabcd));
    EXPECT_THAT((int)enum32, Eq(0x123456));
    EXPECT_THAT((int)enumQt16, Eq(0x4321));
    EXPECT_THAT((int)enumQt32, Eq(0xfedcba));
}

TEST(TypesTest, bufferInfoVariantSerialization)
{
    qRegisterMetaType<BufferInfo>("BufferInfo");
    BufferInfo info1(BufferId(42), NetworkId(1), BufferInfo::ChannelBuffer, 0, "#testchannel");
    QVariant v1 = QVariant::fromValue(info1);

    QByteArray data;
    QDataStream out(&data, QIODevice::WriteOnly);
    out.setVersion(QDataStream::Qt_4_2);
    out << v1;

    QDataStream in(data);
    in.setVersion(QDataStream::Qt_4_2);
    QVariant v2;
    in >> v2;

    EXPECT_TRUE(v2.isValid());
    EXPECT_TRUE(v2.canConvert<BufferInfo>());
    BufferInfo info2 = v2.value<BufferInfo>();
    EXPECT_EQ(info2.bufferId(), BufferId(42));
    EXPECT_EQ(info2.networkId(), NetworkId(1));
    EXPECT_EQ(info2.type(), BufferInfo::ChannelBuffer);
    EXPECT_EQ(info2.bufferName(), QStringLiteral("#testchannel"));
}

#include "typestest.moc"
