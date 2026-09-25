import SwiftUI

struct ChatsView: View {
    @Environment(ChatStore.self) private var store
    @State private var query = ""
    @State private var archived = false
    @State private var archivedRevealed = false
    @State private var newChat = false

    private var visible: [Chat] {
        store.chats.filter {
            $0.archived == archived && (query.isEmpty || store.chatTitle($0).localizedStandardContains(query) || $0.preview.localizedStandardContains(query))
        }
    }

    var body: some View {
        @Bindable var store = store
        NavigationStack(path: $store.navigation) {
            List {
                if store.isDemo || !store.connected || store.syncProgress != nil {
                    Label(store.connectionLabel, systemImage: store.isDemo ? "eye" : "arrow.triangle.2.circlepath")
                        .font(.caption).foregroundStyle(.secondary).padding(.vertical, 2)
                        .listRowSeparator(.hidden)
                }
                if store.chats.contains(where: \.archived) && (archivedRevealed || archived || !query.isEmpty) {
                    Button { archived.toggle() } label: {
                        HStack {
                            Label(archived ? "Back to chats" : "Archived", systemImage: archived ? "bubble.left.and.bubble.right" : "archivebox")
                            Spacer()
                            Text("\(store.chats.filter(\.archived).count)").font(.caption).foregroundStyle(.secondary)
                        }
                    }.tint(.primary)
                }
                ForEach(visible) { chat in
                    ChatListRow(chat: chat)
                }
                if !query.isEmpty {
                    Section("Messages in downloaded history") {
                        if store.searching { ProgressView("Searching…") }
                        ForEach(store.searchHits, id: \.searchIdentity) { message in
                            Button { store.navigate(to: message.chat, message: message.id) } label: {
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(store.chats.first { $0.id == message.chat }?.name ?? "Chat").font(.headline)
                                    Text(message.text).font(.subheadline).lineLimit(3).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .refreshable { archivedRevealed = true }
            .listStyle(.plain)
            .overlay {
                if visible.isEmpty && store.searchHits.isEmpty && !store.searching {
                    ContentUnavailableView(query.isEmpty ? "Your chats will appear here" : "No matching chats",
                                           systemImage: "bubble.left.and.bubble.right",
                                           description: Text(query.isEmpty ? "Keep this app open while WhatsApp syncs your recent history." : "Try another name or recent message."))
                        .allowsHitTesting(false)
                }
            }
            .navigationTitle(archived ? "Archived" : "Chats")
            .searchable(text: $query, prompt: "Search chats and messages")
            .task(id: query) {
                store.searchQuery = query
                if query.isEmpty { store.search(""); return }
                do { try await Task.sleep(for: .milliseconds(180)) } catch { return }
                store.search(query)
            }
            .toolbar { ToolbarItem(placement: .topBarTrailing) { Button { newChat = true } label: { Image(systemName: "square.and.pencil") }.accessibilityLabel("New chat") } }
            .sheet(isPresented: $newChat) { NewChatView() }
            .navigationDestination(for: String.self) { ConversationView(chatID: $0) }
        }
    }
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
        Button { store.navigation.append(chat.id) } label: {
            HStack(spacing: 12) {
                AvatarView(name: store.chatTitle(chat), url: store.localURL(store.avatars[chat.id]), group: chat.kind == "group", size: 52)
                VStack(alignment: .leading, spacing: 5) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(store.chatTitle(chat)).font(.system(.body, weight: .semibold)).foregroundStyle(.primary).lineLimit(1)
                        Spacer(minLength: 8)
                        Text(ChatDate.preview(chat.timestamp))
                            .font(.caption2).foregroundStyle(chat.unread > 0 ? Color.accentColor : .secondary).fixedSize()
                    }
                    HStack(alignment: .center, spacing: 5) {
                        Text(store.typing[chat.id]?.isEmpty == false ? store.presenceLabel(chat) ?? chat.preview : chat.preview.isEmpty ? "No messages yet" : chat.preview)
                            .font(.subheadline).foregroundStyle(store.typing[chat.id]?.isEmpty == false ? Color.accentColor : .secondary).lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if chat.pinned { Image(systemName: "pin.fill").font(.system(size: 10)).rotationEffect(.degrees(35)).foregroundStyle(.tertiary).accessibilityLabel("Pinned") }
                        if chat.muted { Image(systemName: "bell.slash.fill").font(.caption2).foregroundStyle(.tertiary) }
                        if chat.unread > 0 {
                            Text(chat.unread > 99 ? "99+" : "\(chat.unread)").font(.system(size: 11, weight: .semibold)).foregroundStyle(Color(.systemBackground))
                                .padding(.horizontal, 5).frame(minWidth: 18, minHeight: 18).background(Color.accentColor, in: Capsule())
                                .accessibilityLabel("\(chat.unread) unread messages")
                        }
                    }
                }
            }.padding(.vertical, 7).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .alignmentGuide(.listRowSeparatorLeading) { _ in 64 }
        .task(id: store.connected) { store.avatar(chat.id) }
        .accessibilityIdentifier("chat-\(chat.id)")
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button(chat.archived ? "Unarchive" : "Archive") { store.setArchived(chat) }.tint(.indigo)
            Button(chat.pinned ? "Unpin" : "Pin") { store.setPinned(chat) }.tint(.orange)
        }
        .swipeActions(edge: .leading, allowsFullSwipe: false) {
            Button("Read") { store.markChatRead(chat) }.tint(.green)
        }
        .contextMenu {
            Button(chat.pinned ? "Unpin chat" : "Pin chat") { store.setPinned(chat) }
            Button(chat.archived ? "Unarchive chat" : "Archive chat") { store.setArchived(chat) }
            ChatMuteMenu(chat: chat)
            Button("Mark as read") { store.markChatRead(chat) }
        }
    }
}
