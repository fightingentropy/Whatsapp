import SwiftUI

struct MessageBubble: View {
    let message: Message
    let group: Bool
    var joinsPrevious = false
    var joinsNext = false
    var selecting = false
    var selected = false
    var select: () -> Void = {}
    let openMedia: (Message) -> Void
    @Environment(ChatStore.self) private var store
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
        let text = messageText
        HStack(alignment: .bottom, spacing: 6) {
            if selecting {
                Button(action: select) { Image(systemName: selected ? "checkmark.circle.fill" : "circle").font(.title3) }.accessibilityLabel(selected ? "Deselect message" : "Select message")
            }
            if message.fromMe { Spacer(minLength: 34) }
            else if group || store.preferences.senderPictures {
                Group {
                    if !joinsNext {
                        AvatarView(name: senderName, url: store.localURL(store.avatars[message.sender]), size: 26)
                            .task { store.avatar(message.sender) }
                    } else { Color.clear.frame(width: 26, height: 26) }
                }.padding(.bottom, message.reactions.isEmpty ? 2 : 25)
            }
            VStack(alignment: message.fromMe ? .trailing : .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 5) {
                    if group && !message.fromMe && !joinsPrevious {
                        Text(senderName).font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(ChatAppearance.senderColor(senderName)).lineLimit(1)
                    }
                    if message.forwarded == true {
                        Label("Forwarded", systemImage: "arrowshape.turn.up.right")
                            .font(.caption2).italic().foregroundStyle(.secondary)
                    }
                    if let quote = message.quote {
                        Button { store.jump(to: quote.id) } label: {
                            HStack(spacing: 8) {
                                RoundedRectangle(cornerRadius: 2).fill(Color.accentColor).frame(width: 3)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(quote.sender.contains("@") ? store.displayName(quote.sender) : quote.sender)
                                        .font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
                                    Text(quote.text).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 7)
                            }.fixedSize(horizontal: false, vertical: true).padding(.trailing, 9)
                                .background(.black.opacity(scheme == .dark ? 0.14 : 0.04), in: RoundedRectangle(cornerRadius: 7))
                        }.buttonStyle(.plain).accessibilityLabel("Jump to quoted message")
                    }
                    if isPlainText {
                        ViewThatFits(in: .horizontal) {
                            HStack(alignment: .bottom, spacing: 8) {
                                text
                                metadata.padding(.bottom, 1)
                            }.fixedSize(horizontal: true, vertical: false)
                            VStack(alignment: .trailing, spacing: 3) { text; metadata }
                        }
                    } else {
                        VStack(alignment: .trailing, spacing: 5) {
                            VStack(alignment: .leading, spacing: 7) {
                                if message.kind != "revoked" { richContent }
                                if message.hasMedia && message.kind != "revoked" { media }
                                if showsText { text }
                            }
                            metadata
                                .padding(.horizontal, isSticker ? 6 : 0).padding(.vertical, isSticker ? 3 : 0)
                                .background(isSticker ? ChatAppearance.incoming : .clear, in: Capsule())
                        }
                    }
                }
                .padding(.horizontal, isSticker ? 0 : 10).padding(.vertical, isSticker ? 2 : 7)
                .background(isSticker ? .clear : bubbleColor, in: bubbleShape)
                .contextMenu { messageMenu }
                .accessibilityIdentifier("message-\(message.id)")
                if !message.reactions.isEmpty {
                    Button { infoPresented = true } label: {
                        Text(message.reactions.joined(separator: " ")).font(.system(size: 15))
                            .padding(.horizontal, 8).padding(.vertical, 3)
                            .background(ChatAppearance.incoming, in: Capsule())
                            .overlay(Capsule().stroke(ChatAppearance.canvas, lineWidth: 2))
                            .padding(.vertical, 4)
                    }.buttonStyle(.plain).padding(.horizontal, 7).padding(.top, -4)
                        .accessibilityLabel("Reactions: \(message.reactions.joined(separator: ", ")). Show details")
                }
            }
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

    private var senderName: String { store.displayName(message.sender, fallback: message.senderName) }
    private var isSticker: Bool { message.kind == "sticker" && message.mediaPath != nil }
    private var showsText: Bool {
        !message.text.isEmpty && !["poll", "contact", "location"].contains(message.content?.kind ?? "") && message.kind != "audio"
    }
    private var isPlainText: Bool { showsText && !message.hasMedia && message.content?.preview == nil && message.quote == nil }
    private var bubbleColor: Color { message.fromMe ? ChatAppearance.outgoing : ChatAppearance.incoming }
    private var bubbleShape: UnevenRoundedRectangle {
        UnevenRoundedRectangle(topLeadingRadius: !message.fromMe && joinsPrevious ? 5 : 16,
                               bottomLeadingRadius: !message.fromMe && joinsNext ? 5 : 16,
                               bottomTrailingRadius: message.fromMe && joinsNext ? 5 : 16,
                               topTrailingRadius: message.fromMe && joinsPrevious ? 5 : 16)
    }
    private var messageText: some View {
        Text(MessageText.render(message.text, mentions: mentions, size: textSize)).textSelection(.enabled)
            .foregroundStyle(message.kind == "revoked" ? .secondary : .primary)
            .fixedSize(horizontal: false, vertical: true)
    }
    private var metadata: some View {
        HStack(spacing: 4) {
            if message.edited { Text("edited") }
            Text(Date(timeIntervalSince1970: message.timestamp), format: .dateTime.hour().minute()).monospacedDigit()
            if message.fromMe { DeliveryMark(status: message.status) }
        }.font(.system(size: 10.5)).foregroundStyle(.secondary).fixedSize()
    }

    @ViewBuilder private var richContent: some View {
        if let content = message.content {
            if let preview = content.preview, let url = URL(string: preview.url), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                Link(destination: url) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(preview.title ?? url.host ?? "Link").font(.subheadline.bold()).lineLimit(2).foregroundStyle(Color.primary)
                        if let description = preview.description { Text(description).font(.caption).lineLimit(3) }
                        Text(url.host ?? preview.url).font(.caption2)
                    }.foregroundStyle(Color.secondary).frame(maxWidth: 260, alignment: .leading).padding(10).background(Color.black.opacity(scheme == .dark ? 0.2 : 0.05), in: RoundedRectangle(cornerRadius: 10))
                }.buttonStyle(.plain)
            }
            if content.kind == "poll" {
                Text(content.question ?? "Poll").font(.body.weight(.semibold))
                Text("Poll · view only").font(.caption2).foregroundStyle(.secondary)
                ForEach(Array((content.options ?? []).enumerated()), id: \.offset) { _, option in
                    Label(option, systemImage: "circle").font(.subheadline).padding(.vertical, 5)
                }
            }
            if content.kind == "location", let latitude = content.latitude, let longitude = content.longitude,
               let url = URL(string: "https://maps.apple.com/?ll=\(latitude),\(longitude)") {
                Link(destination: url) { Label(content.name ?? "Location", systemImage: "map.fill").font(.subheadline.weight(.semibold)).padding(.vertical, 10).padding(.horizontal, 4) }
                if let address = content.address { Text(address).font(.caption) }
            }
            if content.kind == "contact" {
                Button { contactPresented = true } label: { Label(content.display_name ?? "Contact", systemImage: "person.crop.rectangle").font(.subheadline.weight(.semibold)).padding(.vertical, 10).padding(.horizontal, 4) }
            }
        }
    }

    @ViewBuilder private var media: some View {
        if message.kind == "audio", message.mediaPath != nil { VoicePlaybackView(message: message, audio: store.audio) }
        else {
            Button { openMedia(message) } label: {
                if let url = store.localURL(message.mediaPath), message.kind == "sticker" || message.content?.gif == true {
                    AnimatedMedia(url: url, video: message.content?.gif == true).frame(width: isSticker ? 156 : 240, height: isSticker ? 156 : 220).clipShape(RoundedRectangle(cornerRadius: 10))
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

}
