import SwiftUI

struct MessageBubble: View {
    let message: Message
    let group: Bool
    var selecting = false
    var selected = false
    var select: () -> Void = {}
    let openMedia: (Message) -> Void
    @EnvironmentObject private var store: ChatStore
    @Environment(\.colorScheme) private var scheme
    @State private var forwardPresented = false
    @State private var infoPresented = false
    @State private var contactPresented = false
    @State private var deletion: Bool?
    @State private var customReaction = false
    @State private var reaction = ""
    private var mentions: [String: String] { store.mentionNames(message) }
    private var textSize: Double {
        let compact = message.text.filter { !$0.isWhitespace }
        return (1...3).contains(compact.count) && compact.allSatisfy { EmojiCatalog.isEmoji(String($0)) } ? store.preferences.textSize * 2 : store.preferences.textSize
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 6) {
            if selecting {
                Button(action: select) { Image(systemName: selected ? "checkmark.circle.fill" : "circle").font(.title3) }.accessibilityLabel(selected ? "Deselect message" : "Select message")
            }
            if message.fromMe { Spacer(minLength: 34) }
            else if group || store.preferences.senderPictures {
                AvatarView(name: store.displayName(message.sender, fallback: message.senderName), url: store.localURL(store.avatars[message.sender])).scaleEffect(0.55).frame(width: 30, height: 32)
                    .task { store.avatar(message.sender) }
            }
            VStack(alignment: .leading, spacing: 6) {
                if group && !message.fromMe {
                    Text(store.displayName(message.sender, fallback: message.senderName)).font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
                }
                if message.forwarded == true { Label("Forwarded", systemImage: "arrowshape.turn.up.right").font(.caption).foregroundStyle(.secondary) }
                if let quote = message.quote {
                    Button { store.jump(to: quote.id) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(quote.sender.contains("@") ? store.displayName(quote.sender) : quote.sender).font(.caption.bold()).foregroundStyle(Color.accentColor)
                            Text(quote.text).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                        }.frame(maxWidth: .infinity, alignment: .leading).padding(9)
                            .background(.black.opacity(0.08), in: RoundedRectangle(cornerRadius: 9))
                    }.buttonStyle(.plain).accessibilityLabel("Jump to quoted message")
                }
                if message.kind != "revoked" { richContent }
                if message.hasMedia && message.kind != "revoked" { media }
                if !message.text.isEmpty && message.content?.kind != "poll" && message.content?.kind != "contact" && message.content?.kind != "location" && message.kind != "audio" {
                    Text(MessageText.render(message.text, mentions: mentions, size: textSize)).textSelection(.enabled)
                        .foregroundStyle(message.kind == "revoked" ? .secondary : .primary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 4) {
                    Spacer(minLength: 0)
                    if message.edited { Text("edited") }
                    Text(Date(timeIntervalSince1970: message.timestamp), format: .dateTime.hour().minute())
                    if message.fromMe { Image(systemName: statusIcon).foregroundStyle(message.status == "read" || message.status == "played" ? Color.cyan : .secondary).accessibilityLabel(message.status) }
                }.font(.system(size: 11)).foregroundStyle(.secondary)
                if !message.reactions.isEmpty {
                    Button { infoPresented = true } label: { Text(message.reactions.joined(separator: " ")).font(.subheadline) }.buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 11).padding(.vertical, 8)
            .background(bubbleColor, in: RoundedRectangle(cornerRadius: 15))
            .contextMenu { messageMenu }
            .accessibilityIdentifier("message-\(message.id)")
            if !message.fromMe { Spacer(minLength: 34) }
        }
        .task(id: store.connected) { store.autoDownload(message) }
        .sheet(isPresented: $forwardPresented) { ForwardPicker(message: message) }
        .sheet(isPresented: $infoPresented) { NavigationStack { MessageDetails(message: message).toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { infoPresented = false } } } } }
        .sheet(isPresented: $contactPresented) { NavigationStack { ContactCard(vcard: message.content?.vcard ?? "").toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { contactPresented = false } } } } }
        .confirmationDialog(deletion == true ? "Delete this message for everyone?" : "Delete this message from this device?", isPresented: Binding(get: { deletion != nil }, set: { if !$0 { deletion = nil } }), titleVisibility: .visible) {
            if let deletion { Button(deletion ? "Delete for everyone" : "Delete for me", role: .destructive) { store.delete(message, everyone: deletion); self.deletion = nil } }
        }
        .alert("React with an emoji", isPresented: $customReaction) {
            TextField("Emoji", text: $reaction)
            Button("React") { store.react(message, emoji: reaction.trimmingCharacters(in: .whitespacesAndNewlines)) }
                .disabled(!EmojiCatalog.isEmoji(reaction.trimmingCharacters(in: .whitespacesAndNewlines)))
            Button("Cancel", role: .cancel) {}
        }
    }

    private var bubbleColor: Color {
        if message.fromMe { return scheme == .dark ? Color(red: 0.035, green: 0.27, blue: 0.23) : Color(red: 0.84, green: 0.97, blue: 0.78) }
        return Color(.secondarySystemGroupedBackground)
    }

    @ViewBuilder private var richContent: some View {
        if let content = message.content {
            if let preview = content.preview, let url = URL(string: preview.url), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                Link(destination: url) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(preview.title ?? url.host ?? "Link").font(.subheadline.bold()).lineLimit(2).foregroundStyle(Color.primary)
                        if let description = preview.description { Text(description).font(.caption).lineLimit(3) }
                        Text(url.host ?? preview.url).font(.caption2)
                    }.foregroundStyle(Color.secondary).frame(maxWidth: .infinity, alignment: .leading).padding(10).background(Color.black.opacity(scheme == .dark ? 0.2 : 0.05), in: RoundedRectangle(cornerRadius: 10))
                }.buttonStyle(.plain)
            }
            if content.kind == "poll" {
                Text(content.question ?? "Poll").font(.headline)
                ForEach(Array((content.options ?? []).enumerated()), id: \.offset) { _, option in Label(option, systemImage: "circle").font(.body).padding(.vertical, 4) }
            }
            if content.kind == "location", let latitude = content.latitude, let longitude = content.longitude,
               let url = URL(string: "https://maps.apple.com/?ll=\(latitude),\(longitude)") {
                Link(destination: url) { Label(content.name ?? "Location", systemImage: "map.fill").font(.headline).padding(14) }
                if let address = content.address { Text(address).font(.caption) }
            }
            if content.kind == "contact" {
                Button { contactPresented = true } label: { Label(content.display_name ?? "Contact", systemImage: "person.crop.rectangle").font(.headline).padding(14) }
            }
        }
    }

    @ViewBuilder private var media: some View {
        if message.kind == "audio", message.mediaPath != nil { VoicePlaybackView(message: message, audio: store.audio) }
        else {
            Button { openMedia(message) } label: {
                if let url = store.localURL(message.mediaPath), message.kind == "sticker" || message.content?.gif == true {
                    AnimatedMedia(url: url, video: message.content?.gif == true).frame(width: 220, height: 220).clipShape(RoundedRectangle(cornerRadius: 10))
                } else if message.kind == "image", let url = store.localURL(message.mediaPath) {
                    LocalImage(url: url, maximumSize: 720).scaledToFit().frame(maxWidth: 240, maxHeight: 280).clipShape(RoundedRectangle(cornerRadius: 10))
                } else {
                    HStack(spacing: 10) {
                        if message.mediaState == "downloading" { ProgressView() }
                        else { Image(systemName: message.mediaPath != nil ? "doc" : "arrow.down.circle").font(.title2) }
                        VStack(alignment: .leading, spacing: 3) {
                            Text(message.content?.file_name ?? (message.kind == "audio" ? "Voice / audio message" : message.kind.capitalized)).font(.subheadline.weight(.medium)).lineLimit(2)
                            Text(message.mediaError ?? (message.mediaPath == nil ? "Tap to download" : "Tap to open")).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                        }
                    }.padding(.vertical, 8)
                }
            }.buttonStyle(.plain).disabled(message.mediaState == "downloading" || (message.mediaPath == nil && !store.connected))
        }
    }

    @ViewBuilder private var messageMenu: some View {
        if message.kind != "revoked" && store.canPost {
            Menu("React") {
                ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in Button(emoji) { store.react(message, emoji: emoji) } }
                if message.reactionDetails?.contains(where: \.from_me) == true { Button("Remove my reaction") { store.react(message, emoji: "") } }
                Button("Choose another emoji…") { reaction = ""; customReaction = true }
            }
            Button { store.reply = message; store.editing = nil } label: { Label("Reply", systemImage: "arrowshape.turn.up.left") }
        }
        if !message.text.isEmpty { Button { UIPasteboard.general.string = MessageText.plain(message.text, mentions: mentions) } label: { Label("Copy text", systemImage: "doc.on.doc") } }
        Button(action: select) { Label("Select messages", systemImage: "checkmark.circle") }
        if message.kind != "revoked" && message.content?.kind != "unsupported" {
            Button { forwardPresented = true } label: { Label("Forward", systemImage: "arrowshape.turn.up.right") }
        }
        if message.canEdit && store.canPost { Button { store.editing = message } label: { Label("Edit", systemImage: "pencil") } }
        Button { infoPresented = true } label: { Label("Message info", systemImage: "info.circle") }
        if let url = store.localURL(message.mediaPath) {
            ShareLink(item: url) { Label("Share file", systemImage: "square.and.arrow.up") }
            if message.kind == "sticker" { Button("Save sticker") { store.saveSticker(url.path) } }
        }
        if message.canRevoke { Button("Delete for everyone", role: .destructive) { deletion = true } }
        Button("Delete for me", role: .destructive) { deletion = false }
    }

    private var statusIcon: String {
        switch message.status {
        case "pending": return "clock"
        case "failed": return "exclamationmark.circle"
        case "delivered", "read", "played": return "checkmark.circle.fill"
        default: return "checkmark"
        }
    }
}
