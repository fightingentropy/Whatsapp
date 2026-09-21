import SwiftUI

struct ChatsView: View {
    @EnvironmentObject private var store: ChatStore
    @State private var query = ""
    @State private var archived = false

    private var visible: [Chat] {
        store.chats.filter {
            $0.archived == archived && (query.isEmpty || $0.name.localizedStandardContains(query) || $0.preview.localizedStandardContains(query))
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if store.isDemo || !store.connected || store.syncProgress != nil {
                    Label(store.connectionLabel, systemImage: store.isDemo ? "eye" : "arrow.triangle.2.circlepath")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                if store.chats.contains(where: \.archived) {
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
                            AvatarView(name: chat.name, url: store.localURL(store.avatars[chat.id]), group: chat.kind == "group")
                            VStack(alignment: .leading, spacing: 6) {
                                HStack(alignment: .firstTextBaseline) {
                                    Text(chat.name).font(.headline).lineLimit(1)
                                    Spacer(minLength: 8)
                                    Text(Date(timeIntervalSince1970: chat.timestamp), format: .dateTime.hour().minute())
                                        .font(.caption).foregroundStyle(chat.unread > 0 ? Color.accentColor : .secondary)
                                }
                                HStack(alignment: .top) {
                                    Text(chat.preview.isEmpty ? "No messages yet" : chat.preview)
                                        .font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                    if chat.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(.tertiary) }
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
                }
            }
            .listStyle(.plain)
            .overlay {
                if visible.isEmpty {
                    ContentUnavailableView(query.isEmpty ? "Your chats will appear here" : "No matching chats",
                                           systemImage: "bubble.left.and.bubble.right",
                                           description: Text(query.isEmpty ? "Keep this app open while WhatsApp syncs your recent history." : "Try another name or recent message."))
                        .allowsHitTesting(false)
                }
            }
            .navigationTitle(archived ? "Archived" : "Chats")
            .searchable(text: $query, prompt: "Search chats")
            .navigationDestination(for: String.self) { ConversationView(chatID: $0) }
        }
    }
}

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
