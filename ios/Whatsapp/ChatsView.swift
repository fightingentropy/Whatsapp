import SwiftUI

struct ChatsView: View {
    @Environment(ChatStore.self) private var store
    @State private var query = ""
    @State private var archived = false
    @State private var archivedRevealed = false
    @State private var newChat = false
    /// A reconnect that finishes quickly shows nothing; the status row
    /// appears only when connecting takes noticeably long.
    @State private var slowConnection = false

    private var visible: [Chat] {
        store.chats.filter {
            $0.archived == archived && (query.isEmpty || store.chatTitle($0).localizedStandardContains(query) || $0.preview.localizedStandardContains(query))
        }
    }

    var body: some View {
        @Bindable var store = store
        NavigationStack(path: $store.navigation) {
            List {
                header.listRowBackground(Theme.bg).listRowSeparator(.hidden)
                if store.chats.contains(where: \.archived) && (archivedRevealed || archived || !query.isEmpty) {
                    Button { archived.toggle() } label: {
                        HStack {
                            Label(archived ? "Back to chats" : "Archived", systemImage: archived ? "bubble.left.and.bubble.right" : "archivebox")
                                .foregroundStyle(Theme.fg)
                            Spacer()
                            Text("\(store.chats.filter(\.archived).count)").font(Theme.mono(12)).foregroundStyle(Theme.muted)
                        }
                    }.listRowBackground(Theme.bg).listRowSeparatorTint(Theme.line)
                }
                ForEach(visible) { chat in
                    ChatListRow(chat: chat).listRowBackground(Theme.bg).listRowSeparatorTint(Theme.line)
                }
                if !query.isEmpty {
                    Section {
                        if store.searching { ProgressView("Searching…").listRowBackground(Theme.bg) }
                        ForEach(store.searchHits, id: \.searchIdentity) { message in
                            Button { store.navigate(to: message.chat, message: message.id) } label: {
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(store.chatByID(message.chat)?.name ?? "Chat").font(Theme.sans(15, .semibold)).foregroundStyle(Theme.fg)
                                    Text(message.text).font(Theme.sans(14)).lineLimit(3).foregroundStyle(Theme.muted)
                                }
                            }.listRowBackground(Theme.bg).listRowSeparatorTint(Theme.line)
                        }
                    } header: { Eyebrow("Messages in downloaded history") }
                }
            }
            .refreshable { archivedRevealed = true }
            .task(id: store.connected) {
                slowConnection = false
                guard !store.connected else { return }
                do { try await Task.sleep(for: ChatsView.connectionPatience) } catch { return }
                slowConnection = true
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(Theme.bg.ignoresSafeArea())
            .scrollDismissesKeyboard(.immediately)
            .overlay {
                if store.chatsLoaded && visible.isEmpty && store.searchHits.isEmpty && !store.searching {
                    ContentUnavailableView(query.isEmpty ? "Your chats will appear here" : "No matching chats",
                                           systemImage: "bubble.left.and.bubble.right",
                                           description: Text(query.isEmpty ? "Keep this app open while WhatsApp syncs your recent history." : "Try another name or recent message."))
                        .foregroundStyle(Theme.muted)
                        .allowsHitTesting(false)
                }
            }
            .navigationTitle(archived ? "Archived" : "Chats")
            .toolbar(.hidden, for: .navigationBar)
            .task(id: query) {
                store.searchQuery = query
                if query.isEmpty { store.search(""); return }
                do { try await Task.sleep(for: .milliseconds(180)) } catch { return }
                store.search(query)
            }
            .sheet(isPresented: $newChat) { NewChatView() }
            .navigationDestination(for: String.self) { ConversationView(chatID: $0) }
        }
    }

    /// Rota's page header: wordmark and action, a status line, then search.
    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                Wordmark(text: archived ? "archived" : "chats").accessibilityIdentifier("chats-title")
                Spacer()
                Button { newChat = true } label: { Image(systemName: "square.and.pencil") }
                    .buttonStyle(PillStyle(kind: .secondary))
                    .accessibilityLabel("New chat")
            }
            if store.isDemo || store.syncProgress != nil || (!store.connected && slowConnection) {
                HStack(spacing: 6) {
                    Dot(color: store.isDemo ? Theme.muted : (store.syncProgress != nil ? Theme.green : Theme.amber))
                    Text(store.connectionLabel).font(Theme.mono(12.5)).foregroundStyle(Theme.muted)
                }
            }
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(.system(size: 14, weight: .medium)).foregroundStyle(Theme.muted)
                TextField("Search chats and messages", text: $query)
                    .font(Theme.sans(15)).foregroundStyle(Theme.fg)
                    .textInputAutocapitalization(.never).autocorrectionDisabled().submitLabel(.search)
                    .accessibilityIdentifier("chats-search")
                if !query.isEmpty {
                    Button { query = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Theme.faint) }
                        .buttonStyle(.plain).accessibilityLabel("Clear search")
                }
            }
            .padding(.horizontal, 12).frame(height: 40)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
        .textCase(nil)
        .padding(.top, 8).padding(.bottom, 6)
        .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16))
    }
}

extension ChatsView {
    /// Typical reconnects after reopening finish well within this.
    static let connectionPatience: Duration = .seconds(2)
}

extension Message { var searchIdentity: String { chat + ":" + id } }

struct AvatarView: View {
    let name: String
    var url: URL? = nil
    var group = false
    var size: CGFloat = 54
    private var color: Color { group ? .secondary : ChatAppearance.senderColor(name) }

    var body: some View {
        ZStack {
            Circle().fill(color.opacity(0.14))
            if let url { LocalImage(url: url, maximumSize: max(120, Int(size * 3))).scaledToFill() }
            else if group { Image(systemName: "person.2.fill").font(.system(size: size * 0.34, weight: .medium)).foregroundStyle(color) }
            else {
                Text(name.split(separator: " ").prefix(2).compactMap(\.first).map(String.init).joined())
                    .font(.system(size: size * 0.34, weight: .medium, design: .rounded)).foregroundStyle(color)
            }
        }.frame(width: size, height: size).clipShape(Circle()).accessibilityHidden(true)
    }
}

/// Row-specific observation keeps avatar/typing updates out of the list container.
private struct ChatListRow: View {
    @Environment(ChatStore.self) private var store
    let chat: Chat
    var body: some View {
        let title = store.chatTitle(chat)
        let typing = store.isTyping(in: chat.id)
        Button { store.navigation.append(chat.id) } label: {
            HStack(spacing: 12) {
                AvatarView(name: title, url: store.avatarURL(chat.id), group: chat.kind == "group", size: 50)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(title).font(Theme.sans(16, .semibold)).foregroundStyle(Theme.fg).lineLimit(1)
                        Spacer(minLength: 8)
                        Text(ChatDate.preview(chat.timestamp))
                            .font(Theme.mono(11.5, chat.unread > 0 ? .semibold : .regular))
                            .foregroundStyle(chat.unread > 0 ? Theme.fg : Theme.muted).fixedSize()
                    }
                    HStack(alignment: .center, spacing: 5) {
                        Text(typing ? store.presenceLabel(chat) ?? chat.preview : chat.preview.isEmpty ? "No messages yet" : chat.preview)
                            .font(Theme.sans(14)).foregroundStyle(typing ? Theme.green : Theme.muted).lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if chat.pinned { Image(systemName: "pin.fill").font(.system(size: 10)).rotationEffect(.degrees(35)).foregroundStyle(Theme.faint).accessibilityLabel("Pinned") }
                        if chat.muted { Image(systemName: "bell.slash.fill").font(.caption2).foregroundStyle(Theme.faint) }
                        if chat.unread > 0 {
                            Text(chat.unread > 99 ? "99+" : "\(chat.unread)").font(Theme.mono(11, .semibold)).foregroundStyle(Theme.bg)
                                .padding(.horizontal, 5).frame(minWidth: 19, minHeight: 19)
                                .background(chat.muted ? Theme.muted : Theme.fg, in: Capsule())
                                .accessibilityLabel("\(chat.unread) unread messages")
                        }
                    }
                }
            }.padding(.vertical, 6).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .alignmentGuide(.listRowSeparatorLeading) { _ in 62 }
        .task(id: store.connected) { store.avatar(chat.id) }
        .accessibilityIdentifier("chat-\(chat.id)")
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button(chat.archived ? "Unarchive" : "Archive") { store.setArchived(chat) }.tint(Theme.muted)
            Button(chat.pinned ? "Unpin" : "Pin") { store.setPinned(chat) }.tint(Theme.amber)
        }
        .swipeActions(edge: .leading, allowsFullSwipe: false) {
            Button("Read") { store.markChatRead(chat) }.tint(Theme.green)
        }
        .contextMenu {
            Button(chat.pinned ? "Unpin chat" : "Pin chat") { store.setPinned(chat) }
            Button(chat.archived ? "Unarchive chat" : "Archive chat") { store.setArchived(chat) }
            ChatMuteMenu(chat: chat)
            Button("Mark as read") { store.markChatRead(chat) }
        }
    }
}
