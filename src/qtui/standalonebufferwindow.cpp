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

#include "standalonebufferwindow.h"

#include <QBoxLayout>
#include <QCloseEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QSplitter>
#include <QVBoxLayout>
#include <QWidget>

#include "chatview.h"
#include "client.h"
#include "icon.h"
#include "multilineedit.h"
#include "network.h"
#include "networkmodel.h"
#include "nickview.h"
#include "nickviewfilter.h"
#include "tabcompleter.h"

StandaloneBufferWindow::StandaloneBufferWindow(BufferId bufferId, QWidget* parent)
    : QMainWindow(parent)
    , _bufferId(bufferId)
{
    setAttribute(Qt::WA_DeleteOnClose);
    _bufferInfo = Client::networkModel() ? Client::networkModel()->bufferInfo(bufferId) : BufferInfo();

    auto* centralWidget = new QWidget(this);
    auto* layout = new QVBoxLayout(centralWidget);
    layout->setContentsMargins(6, 6, 6, 6);
    layout->setSpacing(6);

    // Topic bar & Users toggle button
    auto* topLayout = new QHBoxLayout();
    topLayout->setContentsMargins(0, 0, 0, 0);
    topLayout->setSpacing(6);

    _topicLabel = new QLabel(this);
    _topicLabel->setObjectName("PoppedOutTopic");
    _topicLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
    _topicLabel->setWordWrap(false);
    _topicLabel->setStyleSheet(QStringLiteral("padding: 4px 8px; font-weight: bold; background: #181825; border-radius: 4px;"));
    topLayout->addWidget(_topicLabel, 1);

    _toggleUsersButton = new QPushButton(icon::get({"im-user", "system-users"}), tr("Users"), this);
    _toggleUsersButton->setObjectName("PoppedOutUsersButton");
    _toggleUsersButton->setCheckable(true);
    _toggleUsersButton->setChecked(false);
    _toggleUsersButton->setToolTip(tr("Show/hide user list"));
    _toggleUsersButton->setStyleSheet(QStringLiteral("padding: 4px 10px; font-weight: bold; border-radius: 4px;"));
    topLayout->addWidget(_toggleUsersButton);

    layout->addLayout(topLayout);

    // Splitter with ChatView and NickView
    _splitter = new QSplitter(Qt::Horizontal, centralWidget);

    _chatView = new ChatView(bufferId, this);
    _splitter->addWidget(_chatView);

    _nickView = new NickView(this);
    if (Client::networkModel()) {
        _nickViewFilter = new NickViewFilter(bufferId, Client::networkModel());
        _nickView->setModel(_nickViewFilter);
        QModelIndex source_current = Client::networkModel()->bufferIndex(bufferId);
        _nickView->setRootIndex(_nickViewFilter->mapFromSource(source_current));
    }
    _nickView->setVisible(false);
    _splitter->addWidget(_nickView);

    _splitter->setStretchFactor(0, 4);
    _splitter->setStretchFactor(1, 1);
    layout->addWidget(_splitter, 1);

    // Bottom input bar
    auto* inputLayout = new QHBoxLayout();
    inputLayout->setContentsMargins(0, 0, 0, 0);
    inputLayout->setSpacing(6);

    _inputEdit = new MultiLineEdit(this);
    _inputEdit->setMode(MultiLineEdit::SingleLine);
    _inputEdit->setPasteProtectionEnabled(true);
    _inputEdit->setPlaceholderText(tr("Message %1...").arg(_bufferInfo.bufferName()));

    _tabCompleter = new TabCompleter(_inputEdit, bufferId);

    _sendButton = new QPushButton(icon::get({"mail-message-new", "go-next"}), tr("Send"), this);
    _sendButton->setStyleSheet(QStringLiteral("padding: 4px 12px; font-weight: bold; border-radius: 6px;"));

    inputLayout->addWidget(_inputEdit, 1);
    inputLayout->addWidget(_sendButton);
    layout->addLayout(inputLayout);

    setCentralWidget(centralWidget);
    resize(760, 540);

    updateTitleAndTopic();

    connect(_inputEdit, &MultiLineEdit::textEntered, this, &StandaloneBufferWindow::sendInput);
    connect(_sendButton, &QPushButton::clicked, this, [this]() {
        if (!_inputEdit->text().isEmpty()) {
            sendInput(_inputEdit->text());
        }
    });
    connect(_toggleUsersButton, &QPushButton::toggled, this, &StandaloneBufferWindow::setUsersVisible);

    if (Client::networkModel()) {
        connect(Client::networkModel(), &QAbstractItemModel::dataChanged, this, [this](const QModelIndex& topLeft, const QModelIndex& bottomRight) {
            Q_UNUSED(topLeft);
            Q_UNUSED(bottomRight);
            updateTitleAndTopic();
        });
    }
}

StandaloneBufferWindow::~StandaloneBufferWindow() = default;

void StandaloneBufferWindow::updateTitleAndTopic()
{
    if (!Client::networkModel())
        return;

    _bufferInfo = Client::networkModel()->bufferInfo(_bufferId);
    const Network* net = Client::network(_bufferInfo.networkId());
    QString netName = net ? net->networkName() : QString();

    QString title = tr("%1 (%2) — Quassel IRC").arg(_bufferInfo.bufferName(), netName);
    setWindowTitle(title);
    setWindowIcon(icon::get("irc-channel-active"));

    // Update topic
    QModelIndex idx = Client::networkModel()->bufferIndex(_bufferId);
    QString topic = idx.sibling(idx.row(), 1).data().toString();
    if (topic.isEmpty()) {
        _topicLabel->setText(tr("<b>%1</b> on %2").arg(_bufferInfo.bufferName(), netName));
    }
    else {
        _topicLabel->setText(tr("<b>%1</b>: %2").arg(_bufferInfo.bufferName(), topic));
    }

    bool isChannel = (_bufferInfo.type() == BufferInfo::ChannelBuffer);
    _toggleUsersButton->setVisible(isChannel);
    if (!isChannel && _nickView) {
        _nickView->setVisible(false);
    }
}

void StandaloneBufferWindow::setUsersVisible(bool visible)
{
    if (_nickView) {
        _nickView->setVisible(visible);
    }
    if (_toggleUsersButton && _toggleUsersButton->isChecked() != visible) {
        _toggleUsersButton->setChecked(visible);
    }
}

bool StandaloneBufferWindow::areUsersVisible() const
{
    return _nickView && !_nickView->isHidden();
}

void StandaloneBufferWindow::toggleUsers()
{
    setUsersVisible(!areUsersVisible());
}

void StandaloneBufferWindow::sendInput(const QString& text)
{
    if (text.isEmpty())
        return;

    Client::userInput(_bufferInfo, text);
    _inputEdit->clear();
}

void StandaloneBufferWindow::closeEvent(QCloseEvent* event)
{
    event->accept();
}
