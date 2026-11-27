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

#pragma once

#include "qtui-export.h"

#include <QLabel>
#include <QMainWindow>
#include <QPointer>

#include "bufferinfo.h"
#include "types.h"

class ChatView;
class MultiLineEdit;
class TabCompleter;
class QPushButton;

class QTUI_EXPORT StandaloneBufferWindow : public QMainWindow
{
    Q_OBJECT

public:
    explicit StandaloneBufferWindow(BufferId bufferId, QWidget* parent = nullptr);
    ~StandaloneBufferWindow() override;

    BufferId bufferId() const { return _bufferId; }
    BufferInfo bufferInfo() const { return _bufferInfo; }

public slots:
    void updateTitleAndTopic();

private slots:
    void sendInput(const QString& text);

protected:
    void closeEvent(QCloseEvent* event) override;

private:
    BufferId _bufferId;
    BufferInfo _bufferInfo;
    ChatView* _chatView{nullptr};
    QLabel* _topicLabel{nullptr};
    MultiLineEdit* _inputEdit{nullptr};
    QPushButton* _sendButton{nullptr};
    TabCompleter* _tabCompleter{nullptr};
};
