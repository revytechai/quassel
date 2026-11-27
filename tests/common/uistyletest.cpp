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

#include <QAbstractItemView>
#include <QColor>
#include <QComboBox>
#include <QFile>
#include <QMenu>
#include <QPalette>
#include <QString>
#include <QTextStream>

#include "buffermodel.h"
#include "client.h"
#include "contextmenuactionprovider.h"
#include "graphicalui.h"
#include "integrationtestsupport.h"
#include "qssparser.h"
#include "qtui.h"
#include "quassel.h"
#include "testglobal.h"
#include "uisettings.h"
#include "uistyle.h"
#include "icon.h"

TEST(UiStyleTest, availableThemesList)
{
    const auto themes = UiStyle::availableThemes();
    EXPECT_GE(themes.size(), 7);

    QStringList ids;
    for (const auto& theme : themes) {
        EXPECT_FALSE(theme.id.isEmpty());
        EXPECT_FALSE(theme.name.isEmpty());
        ids << theme.id;
    }

    EXPECT_TRUE(ids.contains("default"));
    EXPECT_TRUE(ids.contains("dark"));
    EXPECT_TRUE(ids.contains("tokyonight"));
    EXPECT_TRUE(ids.contains("darkmonokai"));
    EXPECT_TRUE(ids.contains("darksolarized"));
    EXPECT_TRUE(ids.contains("classicdark"));
    EXPECT_TRUE(ids.contains("custom"));
}

TEST(UiStyleTest, themeSwitching)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    UiStyleSettings settings;

    // Switch to dark
    UiStyle::setTheme(QStringLiteral("dark"));
    EXPECT_EQ(UiStyle::currentTheme(), QStringLiteral("dark"));
    EXPECT_FALSE(settings.value("UseCustomStyleSheet", false).toBool());

    // Switch to tokyonight
    UiStyle::setTheme(QStringLiteral("tokyonight"));
    EXPECT_EQ(UiStyle::currentTheme(), QStringLiteral("tokyonight"));
    EXPECT_FALSE(settings.value("UseCustomStyleSheet", false).toBool());

    // Switch to custom
    UiStyle::setTheme(QStringLiteral("custom"));
    EXPECT_EQ(UiStyle::currentTheme(), QStringLiteral("custom"));
    EXPECT_TRUE(settings.value("UseCustomStyleSheet", false).toBool());

    // Switch back to default
    UiStyle::setTheme(QStringLiteral("default"));
    EXPECT_EQ(UiStyle::currentTheme(), QStringLiteral("default"));
    EXPECT_FALSE(settings.value("UseCustomStyleSheet", false).toBool());
}

TEST(UiStyleTest, themeSwitchingNotifiesAndReloads)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    UiStyle style;
    int changedCount = 0;
    QObject::connect(&style, &UiStyle::changed, [&changedCount]() {
        changedCount++;
    });

    GraphicalUi::setUiStyle(&style);

    UiStyle::setTheme(QStringLiteral("dark"));
    EXPECT_GT(changedCount, 0);

    GraphicalUi::setUiStyle(nullptr);
}

TEST(UiStyleTest, qssParserPaletteExtraction)
{
    QString qss = QStringLiteral(
        "Palette {\n"
        "  window: #1e1e2e;\n"
        "  base: #181825;\n"
        "  text: #cdd6f4;\n"
        "  highlight: #89b4fa;\n"
        "  marker-line: #f38ba8;\n"
        "  sender-color-00: #fab387;\n"
        "}\n"
        "QMainWindow { background: #1e1e2e; }\n"
    );

    QssParser parser;
    parser.processStyleSheet(qss);

    // Verify palette colors
    EXPECT_EQ(parser.palette().color(QPalette::Window), QColor(0x1e, 0x1e, 0x2e));
    EXPECT_EQ(parser.palette().color(QPalette::Base), QColor(0x18, 0x18, 0x25));
    EXPECT_EQ(parser.palette().color(QPalette::Text), QColor(0xcd, 0xd6, 0xf4));
    EXPECT_EQ(parser.palette().color(QPalette::Highlight), QColor(0x89, 0xb4, 0xfa));

    // Verify custom roles
    const auto uiPalette = parser.uiStylePalette();
    EXPECT_EQ(uiPalette.at(static_cast<int>(UiStyle::ColorRole::MarkerLine)).color(), QColor(0xf3, 0x8b, 0xa8));
    EXPECT_EQ(uiPalette.at(static_cast<int>(UiStyle::ColorRole::SenderColor00)).color(), QColor(0xfa, 0xb3, 0x87));

    // Palette block should be removed, leaving QMainWindow rule
    EXPECT_FALSE(qss.contains("Palette"));
    EXPECT_TRUE(qss.contains("QMainWindow"));
}

TEST(UiStyleTest, qssParserChatLineAndItemBlocks)
{
    QString qss = QStringLiteral(
        "ChatLine#plain { foreground: #cdd6f4; }\n"
        "ChatListItem[state=\"highlighted\"] { foreground: #f38ba8; }\n"
        "NickListItem[type=\"user\"] { foreground: #89b4fa; }\n"
    );

    QssParser parser;
    parser.processStyleSheet(qss);

    EXPECT_FALSE(parser.formats().isEmpty());
    EXPECT_FALSE(parser.listItemFormats().isEmpty());

    // ChatLine, ChatListItem, NickListItem should be removed from remaining css
    EXPECT_FALSE(qss.contains("ChatLine"));
    EXPECT_FALSE(qss.contains("ChatListItem"));
    EXPECT_FALSE(qss.contains("NickListItem"));
}

TEST(UiStyleTest, qssParserStripsComments)
{
    QString qss = QStringLiteral(
        "// Single line comment\n"
        "/* Multi line\n"
        "   comment */\n"
        "QWidget { color: #ffffff; }\n"
    );

    QssParser parser;
    parser.processStyleSheet(qss);

    EXPECT_FALSE(qss.contains("Single line comment"));
    EXPECT_FALSE(qss.contains("Multi line"));
    EXPECT_TRUE(qss.contains("QWidget { color: #ffffff; }"));
}

TEST(UiStyleTest, modernDarkStylesheetParsable)
{
    QString darkQssPath = Quassel::findDataFilePath(QStringLiteral("stylesheets/dark.qss"));
    if (darkQssPath.isEmpty()) {
        darkQssPath = QStringLiteral(":/data/stylesheets/dark.qss");
    }

    QFile file(darkQssPath);
    if (file.open(QFile::ReadOnly | QFile::Text)) {
        QTextStream stream(&file);
        QString content = stream.readAll();
        file.close();

        EXPECT_FALSE(content.isEmpty());

        QssParser parser;
        parser.processStyleSheet(content);

        EXPECT_EQ(parser.palette().color(QPalette::Window), QColor(0x1e, 0x1e, 0x2e));
        EXPECT_EQ(parser.palette().color(QPalette::Base), QColor(0x18, 0x18, 0x25));
        EXPECT_FALSE(parser.formats().isEmpty());
        EXPECT_FALSE(parser.listItemFormats().isEmpty());
    }
}

TEST(UiStyleTest, brushAndSetBrush)
{
    UiStyle style;
    QBrush markerBrush(QColor("#f38ba8"));
    style.setBrush(UiStyle::ColorRole::MarkerLine, markerBrush);
    EXPECT_EQ(style.brush(UiStyle::ColorRole::MarkerLine).color(), QColor("#f38ba8"));

    QBrush selfBrush(QColor("#89b4fa"));
    style.setBrush(UiStyle::ColorRole::SenderColorSelf, selfBrush);
    EXPECT_EQ(style.brush(UiStyle::ColorRole::SenderColorSelf).color(), QColor("#89b4fa"));
}

TEST(UiStyleTest, styleStringFormatting)
{
    UiStyle::StyledString res = UiStyle::styleString(QStringLiteral("Hello %BWorld%B!"), UiStyle::FormatType::PlainMsg);
    EXPECT_EQ(res.plainText, QStringLiteral("Hello World!"));
    EXPECT_GE(res.formatList.size(), 2u);

    // Escaped percent test
    UiStyle::StyledString esc = UiStyle::styleString(QStringLiteral("100%% guaranteed"), UiStyle::FormatType::PlainMsg);
    EXPECT_EQ(esc.plainText, QStringLiteral("100% guaranteed"));

    // mIRC color test (%Dcf04 specifies foreground color 04)
    UiStyle::StyledString col = UiStyle::styleString(QStringLiteral("%Dcf04Red%Dc- text"), UiStyle::FormatType::PlainMsg);
    EXPECT_EQ(col.plainText, QStringLiteral("Red text"));
}

TEST(UiStyleTest, formatGeneration)
{
    UiStyle style;
    UiStyle::Format boldFormat{UiStyle::FormatType::Bold, {}, {}};
    QTextCharFormat charFmt = style.format(boldFormat, UiStyle::MessageLabel::None);
    EXPECT_EQ(charFmt.fontWeight(), static_cast<int>(QFont::Bold));

    UiStyle::Format italicFormat{UiStyle::FormatType::Italic, {}, {}};
    QTextCharFormat italicFmt = style.format(italicFormat, UiStyle::MessageLabel::None);
    EXPECT_TRUE(italicFmt.fontItalic());

    UiStyle::Format underlineFormat{UiStyle::FormatType::Underline, {}, {}};
    QTextCharFormat underFmt = style.format(underlineFormat, UiStyle::MessageLabel::None);
    EXPECT_TRUE(underFmt.fontUnderline());
}

TEST(UiStyleTest, itemFormatTypeBitwiseAndHash)
{
    UiStyle::ItemFormatType base = UiStyle::ItemFormatType::BufferViewItem;
    UiStyle::ItemFormatType active = UiStyle::ItemFormatType::ActiveBuffer;

    UiStyle::ItemFormatType combined = base | active;
    EXPECT_NE(combined, UiStyle::ItemFormatType::None);

    UiStyle::ItemFormatType assigned = base;
    assigned |= active;
    EXPECT_EQ(combined, assigned);

    uint hash1 = qHash(combined, 0);
    uint hash2 = qHash(assigned, 0);
    EXPECT_EQ(hash1, hash2);
}

TEST(UiStyleTest, iconResolutionWithFallback)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    // Verify key IRC and action icons resolve to valid icons using bundled fallback
    EXPECT_FALSE(icon::get("irc-channel-active").isNull());
    EXPECT_FALSE(icon::get("irc-channel-inactive").isNull());
    EXPECT_FALSE(icon::get("network-connect").isNull());
    EXPECT_FALSE(icon::get("network-disconnect").isNull());
    EXPECT_FALSE(icon::get("irc-operator").isNull());
    EXPECT_FALSE(icon::get("irc-voice").isNull());
    EXPECT_FALSE(icon::get({"im-user-offline", "user-offline"}).isNull());
    EXPECT_FALSE(icon::get({"im-user-online", "im-user", "user-available"}).isNull());
}

TEST(UiStyleTest, enforceSolidBackgroundClearsTranslucency)
{
    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);

    // Test QMenu solid background enforcement
    QMenu menu;
    menu.setAttribute(Qt::WA_TranslucentBackground, true);
    menu.setAutoFillBackground(false);
    QtUi::enforceSolidBackground(&menu);
    EXPECT_FALSE(menu.testAttribute(Qt::WA_TranslucentBackground));
    EXPECT_TRUE(menu.autoFillBackground());

    // Test QComboBox and its popup views
    QComboBox cb;
    cb.addItem("Item 1");
    cb.addItem("Item 2");
    if (cb.view()) {
        cb.view()->setAttribute(Qt::WA_TranslucentBackground, true);
        cb.view()->setAutoFillBackground(false);
        if (cb.view()->parentWidget()) {
            cb.view()->parentWidget()->setAttribute(Qt::WA_TranslucentBackground, true);
            cb.view()->parentWidget()->setAutoFillBackground(false);
        }
    }
    QtUi::enforceSolidBackground(&cb);
    if (cb.view()) {
        EXPECT_FALSE(cb.view()->testAttribute(Qt::WA_TranslucentBackground));
        EXPECT_TRUE(cb.view()->autoFillBackground());
        if (cb.view()->viewport()) {
            EXPECT_FALSE(cb.view()->viewport()->testAttribute(Qt::WA_TranslucentBackground));
            EXPECT_TRUE(cb.view()->viewport()->autoFillBackground());
        }
        if (cb.view()->parentWidget()) {
            EXPECT_FALSE(cb.view()->parentWidget()->testAttribute(Qt::WA_TranslucentBackground));
            EXPECT_TRUE(cb.view()->parentWidget()->autoFillBackground());
        }
    }
}

TEST(UiStyleTest, darkQssHasSolidDropdownAndMenuRules)
{
    QString darkQssPath = Quassel::findDataFilePath(QStringLiteral("stylesheets/dark.qss"));
    if (darkQssPath.isEmpty()) {
        darkQssPath = QStringLiteral(":/data/stylesheets/dark.qss");
    }

    QFile file(darkQssPath);
    if (file.open(QFile::ReadOnly)) {
        QString content = QString::fromUtf8(file.readAll());
        EXPECT_TRUE(content.contains("QComboBox QAbstractItemView"));
        EXPECT_TRUE(content.contains("background: #1e1e2e") || content.contains("background-color: #1e1e2e"));
        // Ensure no border or padding is set directly on QComboBox QAbstractItemView which triggers Qt transparency bug
        int pos = content.indexOf("QComboBox QAbstractItemView {");
        if (pos != -1) {
            int end = content.indexOf("}", pos);
            QString block = content.mid(pos, end - pos);
            EXPECT_FALSE(block.contains("border:"));
            EXPECT_FALSE(block.contains("padding:"));
        }
    }
}

TEST(UiStyleTest, styleStringTruncatedFormatSafety)
{
    // Test that malformed truncated formatting codes do not cause out-of-bounds crashes
    const QStringList malformedInputs = {
        "%",
        "%D",
        "%Dc",
        "%Dcf",
        "%Dcf1",
        "%Dh",
        "%Dh#",
        "%Dh#12345",
        "Hello %",
        "Trailing %D",
        "Truncated %Dc"
    };

    for (const auto& input : malformedInputs) {
        EXPECT_NO_THROW({
            UiStyle::StyledString result = UiStyle::styleString(input, UiStyle::FormatType::PlainMsg);
            Q_UNUSED(result);
        });
    }
}

TEST(UiStyleTest, bufferPopOutActionAndModel)
{
    EXPECT_EQ(NetworkModelController::BufferPopOut, 0x50);
    ContextMenuActionProvider provider;
    Action* act = provider.action(NetworkModelController::BufferPopOut);
    ASSERT_NE(act, nullptr);
    EXPECT_TRUE(act->text().contains("Pop Out"));

    test::QuasselTestSupport::ensureInitialized(Quassel::ClientOnly);
    auto* client = new Client(test::QuasselTestSupport::createTestUi());

    BufferModel model;
    bool signalEmitted = false;
    BufferId targetId(42);

    QObject::connect(&model, &BufferModel::bufferPopOutRequested, [&signalEmitted, targetId](const BufferId& id) {
        if (id == targetId)
            signalEmitted = true;
    });

    model.popOutBuffer(targetId);
    EXPECT_TRUE(signalEmitted);

    delete client;
}
