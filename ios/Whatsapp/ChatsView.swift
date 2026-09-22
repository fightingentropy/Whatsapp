import SwiftUI

struct ChatsView: View {
    @EnvironmentObject private var store: ChatStore
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
        NavigationStack(path: $store.navigation) {
            List {
                if store.isDemo || !store.connected || store.syncProgress != nil {
                    Label(store.connectionLabel, systemImage: store.isDemo ? "eye" : "arrow.triangle.2.circlepath")
                        .font(.footnote).foregroundStyle(.secondary)
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
                    NavigationLink(value: chat.id) {
                        HStack(spacing: 13) {
                            AvatarView(name: store.chatTitle(chat), url: store.localURL(store.avatars[chat.id]), group: chat.kind == "group")
                            VStack(alignment: .leading, spacing: 6) {
                                HStack(alignment: .firstTextBaseline) {
                                    Text(store.chatTitle(chat)).font(.headline).lineLimit(1)
                                    Spacer(minLength: 8)
                                    Text(Date(timeIntervalSince1970: chat.timestamp), format: .dateTime.hour().minute())
                                        .font(.caption).foregroundStyle(chat.unread > 0 ? Color.accentColor : .secondary)
                                }
                                HStack(alignment: .top) {
                                    Text(store.typing[chat.id]?.isEmpty == false ? store.presenceLabel(chat) ?? chat.preview : chat.preview.isEmpty ? "No messages yet" : chat.preview)
                                        .font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                    if chat.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(.tertiary) }
                                    if chat.muted { Image(systemName: "bell.slash.fill").font(.caption2).foregroundStyle(.tertiary) }
                                    if chat.unread > 0 {
                                        Text(chat.unread > 99 ? "99+" : "\(chat.unread)").font(.caption2.bold()).foregroundStyle(.black)
                                            .padding(.horizontal, 6).padding(.vertical, 3).background(Color.accentColor, in: Capsule())
                                    }
                                }
                            }
                        }.padding(.vertical, 5)
                    }
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

    var body: some View {
        ZStack {
            Circle().fill(Color.accentColor.opacity(0.13))
            if let url { LocalImage(url: url, maximumSize: 120).scaledToFill() }
            else if group { Image(systemName: "person.2.fill").font(.title3).foregroundStyle(Color.accentColor) }
            else {
                Text(name.split(separator: " ").prefix(2).compactMap(\.first).map(String.init).joined())
                    .font(.headline).foregroundStyle(Color.accentColor)
            }
        }.frame(width: 54, height: 54).clipShape(Circle()).accessibilityHidden(true)
    }
}
