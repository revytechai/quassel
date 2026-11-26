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

#include <QColor>
#include <QFile>
#include <QPalette>
#include <QString>
#include <QTextStream>

#include "integrationtestsupport.h"
#include "qssparser.h"
#include "quassel.h"
#include "testglobal.h"
#include "uisettings.h"
#include "uistyle.h"

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
