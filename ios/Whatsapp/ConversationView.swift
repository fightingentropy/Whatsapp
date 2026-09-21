import QuickLook
import SwiftUI

struct ConversationView: View {
    let chatID: String
    @EnvironmentObject private var store: ChatStore
    @Environment(\.scenePhase) private var phase
    @State private var nearBottom = true
    @State private var didInitialScroll = false
    @State private var historyAnchor: String?
    @State private var previewURL: URL?
    @State private var draftText = ""
    @State private var sending = false

    private var chat: Chat? { store.chats.first { $0.id == store.canonical(chatID) } }
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 7) {
                    if store.loading || store.fetchingPhone {
                        ProgressView(store.fetchingPhone ? "Asking your phone for history…" : "Loading messages…")
                            .font(.caption).padding(16)
                    } else if !(store.archiveComplete && store.phoneComplete) {
                        Button("Load earlier messages") {
                            historyAnchor = store.messages.first?.id
                            store.loadOlder()
                        }.font(.caption).padding(12)
                    }
                    if store.isDemo {
                        Text("Offline preview · messages here are never sent")
                            .font(.caption).foregroundStyle(.secondary).padding(8)
                    }
                    ForEach(Array(store.messages.enumerated()), id: \.element.id) { index, message in
                        if startsDay(index) {
                            Text(Date(timeIntervalSince1970: message.timestamp), format: .dateTime.day().month(.wide))
                                .font(.caption.weight(.medium)).foregroundStyle(.secondary)
                                .padding(.horizontal, 12).padding(.vertical, 6)
                                .background(Color(.secondarySystemBackground), in: Capsule()).padding(.vertical, 9)
                        }
                        MessageBubble(message: message, group: chat?.kind == "group") { message in
                            if let url = store.localURL(message.mediaPath) { previewURL = url }
                            else { store.download(message) }
                        }
                        .id(message.id)
                    }
                    Color.clear.frame(height: 1).id("conversation-bottom")
                }.padding(.horizontal, 12).padding(.bottom, 8)
            }
            .background(Color(red: 0.055, green: 0.073, blue: 0.08))
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .defaultScrollAnchor(.bottom, for: .alignment)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentSize.height - geometry.visibleRect.maxY < 100
            } action: { _, value in nearBottom = value }
            .onChange(of: store.messages.last?.id) { _, _ in
                guard !store.messages.isEmpty else { return }
                if !didInitialScroll || nearBottom || store.messages.last?.fromMe == true {
                    didInitialScroll = true
                    DispatchQueue.main.async { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                }
            }
            .onChange(of: store.messages.first?.id) { _, _ in
                if let historyAnchor {
                    DispatchQueue.main.async { proxy.scrollTo(historyAnchor, anchor: .top) }
                    self.historyAnchor = nil
                }
            }
            .overlay {
                if store.messages.isEmpty && !store.loading && !store.fetchingPhone {
                    ContentUnavailableView("No messages yet", systemImage: "bubble.left",
                                           description: Text("Messages will appear as your phone shares this chat's history."))
                        .allowsHitTesting(false)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if !nearBottom && !store.messages.isEmpty {
                    Button { withAnimation { proxy.scrollTo("conversation-bottom", anchor: .bottom) } } label: {
                        Image(systemName: "chevron.down").font(.headline).padding(14)
                            .background(.regularMaterial, in: Circle())
                    }.padding(14).accessibilityLabel("Jump to latest message")
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { composer }
        }
        .navigationTitle(chat?.name ?? "Chat")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .quickLookPreview($previewURL)
        .onAppear {
            draftText = store.drafts[store.canonical(chatID), default: ""]
            store.open(chatID)
        }
        .onDisappear {
            store.drafts[store.canonical(chatID)] = draftText
            store.close(chatID)
        }
        .onChange(of: phase) { _, phase in
            if phase != .active { store.drafts[store.canonical(chatID)] = draftText }
        }
    }

    private func startsDay(_ index: Int) -> Bool {
        guard index > 0, store.messages.indices.contains(index) else { return true }
        return !Calendar.current.isDate(Date(timeIntervalSince1970: store.messages[index - 1].timestamp),
                                       inSameDayAs: Date(timeIntervalSince1970: store.messages[index].timestamp))
    }

    private var composer: some View {
        VStack(spacing: 8) {
            if !store.connected && !store.isDemo {
                Text("Reconnecting… You can keep writing your draft.").font(.caption).foregroundStyle(.secondary)
            }
            if let reply = store.reply {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Replying to \(reply.fromMe ? "yourself" : (reply.senderName ?? "message"))").font(.caption.bold()).foregroundStyle(Color.accentColor)
                        Text(reply.text).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    Button { store.reply = nil } label: { Image(systemName: "xmark.circle.fill") }.accessibilityLabel("Cancel reply")
                }.padding(10).background(Color(.tertiarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
            }
            if chat?.readOnly == true {
                Text("Only admins can send messages here").font(.footnote).foregroundStyle(.secondary).padding(10)
            } else {
                HStack(alignment: .bottom, spacing: 9) {
                    TextField("Message", text: $draftText, axis: .vertical)
                        .lineLimit(1...5).padding(.horizontal, 15).padding(.vertical, 11)
                        .background(Color(.tertiarySystemBackground), in: RoundedRectangle(cornerRadius: 23))
                        .accessibilityIdentifier("message-composer")
                    Button {
                        sending = true
                        let text = draftText
                        store.sendText(text) { accepted in
                            sending = false
                            if accepted && draftText == text { draftText = "" }
                        }
                    } label: {
                        Image(systemName: "arrow.up").font(.title3.bold()).foregroundStyle(.black)
                            .frame(width: 44, height: 44).background(Color.accentColor, in: Circle())
                    }.disabled(sending || draftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || (!store.connected && !store.isDemo))
                        .accessibilityLabel("Send message").accessibilityIdentifier("send-message")
                }
            }
        }.padding(.horizontal, 12).padding(.vertical, 8).background(.bar)
    }
}

private struct MessageBubble: View {
    let message: Message
    let group: Bool
    let openMedia: (Message) -> Void
    @EnvironmentObject private var store: ChatStore

    var body: some View {
        HStack(alignment: .bottom, spacing: 0) {
            if message.fromMe { Spacer(minLength: 44) }
            VStack(alignment: .leading, spacing: 6) {
                if group && !message.fromMe {
                    Text(store.contactNames[message.sender] ?? message.senderName ?? "Participant")
                        .font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
                }
                if let quote = message.quote {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(quote.sender.contains("@") ? "Reply" : quote.sender).font(.caption.bold()).foregroundStyle(Color.accentColor)
                        Text(quote.text).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(9)
                        .background(.black.opacity(0.13), in: RoundedRectangle(cornerRadius: 9))
                }
                if message.hasMedia {
                    Button { openMedia(message) } label: {
                        if (message.kind == "image" || message.kind == "sticker"), let url = store.localURL(message.mediaPath) {
                            LocalImage(url: url, maximumSize: 720).scaledToFit()
                                .frame(maxWidth: 240, maxHeight: 280).clipShape(RoundedRectangle(cornerRadius: 10))
                        } else {
                            HStack(spacing: 10) {
                                if message.mediaState == "downloading" { ProgressView() }
                                else { Image(systemName: message.mediaPath != nil ? "doc" : "arrow.down.circle").font(.title2) }
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(mediaLabel).font(.subheadline.weight(.medium))
                                    Text(message.mediaState == "failed" ? "Download failed · tap to retry" : (message.mediaPath == nil ? "Tap to download" : "Tap to open"))
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                            }.padding(.vertical, 8)
                        }
                    }.buttonStyle(.plain).disabled(message.mediaState == "downloading" || (message.mediaPath == nil && !store.connected))
                }
                if !message.text.isEmpty {
                    Text(verbatim: message.text).font(.body).textSelection(.enabled)
                        .foregroundStyle(message.kind == "revoked" ? .secondary : .primary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 4) {
                    Spacer(minLength: 0)
                    if message.edited { Text("edited") }
                    Text(Date(timeIntervalSince1970: message.timestamp), format: .dateTime.hour().minute())
                    if message.fromMe {
                        Image(systemName: statusIcon).foregroundStyle(message.status == "read" || message.status == "played" ? Color.cyan : .secondary)
                            .accessibilityLabel(message.status)
                    }
                }.font(.system(size: 11)).foregroundStyle(.secondary)
                if !message.reactions.isEmpty { Text(message.reactions.joined(separator: " ")).font(.subheadline) }
            }
            .padding(.horizontal, 11).padding(.vertical, 8)
            .background(message.fromMe ? Color(red: 0.035, green: 0.27, blue: 0.23) : Color(red: 0.14, green: 0.17, blue: 0.18), in: RoundedRectangle(cornerRadius: 15))
            .contextMenu {
                if !message.text.isEmpty { Button { UIPasteboard.general.string = message.text } label: { Label("Copy", systemImage: "doc.on.doc") } }
                if message.kind != "revoked" && store.currentChat?.readOnly != true {
                    Button { store.reply = message } label: { Label("Reply", systemImage: "arrowshape.turn.up.left") }
                }
            }
            if !message.fromMe { Spacer(minLength: 44) }
        }
    }

    private var mediaLabel: String {
        switch message.kind {
        case "image": return "Photo"
        case "video": return "Video"
        case "audio": return "Audio file"
        case "sticker": return "Sticker"
        default: return "Document"
        }
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
