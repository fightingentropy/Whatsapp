import QuickLook
import SwiftUI

struct ConversationView: View {
    private struct Viewport: Equatable {
        let height: CGFloat
        let nearBottom: Bool
        let rect: CGRect
    }
    let chatID: String
    @Environment(ChatStore.self) private var store
    @Environment(\.scenePhase) private var phase
    @State private var nearBottom = true
    @State private var didInitialScroll = false
    @State private var historyAnchor: String?
    @State private var previewURL: URL?
    @State private var draftText = ""
    @State private var infoPresented = false
    @State private var selection = Set<String>()
    @State private var selecting = false

    private var pageBounds: String { (store.messages.first?.id ?? "") + ":" + (store.messages.last?.id ?? "") }
    private var chat: Chat? { store.chats.first { $0.id == store.canonical(chatID) } }
    var body: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                ScrollView {
                    LazyVStack(spacing: 3) {
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
                            // One stable child per message lets LazyVStack determine
                            // row identities without evaluating every conditional
                            // day separator and bubble in a long conversation.
                            VStack(spacing: 3) {
                                if startsDay(index) {
                                    Text(ChatDate.day(message.timestamp))
                                        .font(.caption2.weight(.medium)).foregroundStyle(.secondary)
                                        .padding(.horizontal, 10).padding(.vertical, 5)
                                        .background(.thinMaterial, in: Capsule()).padding(.vertical, 14)
                                }
                                MessageBubble(message: message, group: chat?.kind == "group", joinsPrevious: joinsPrevious(index), joinsNext: joinsNext(index), selecting: selecting, selected: selection.contains(message.id), select: { selecting = true; if !selection.insert(message.id).inserted { selection.remove(message.id) } }) { message in
                                    if let url = store.localURL(message.mediaPath) { previewURL = url }
                                    else { store.download(message) }
                                }
                                .padding(.top, startsDay(index) || joinsPrevious(index) ? 0 : 7)
                            }
                            .id(message.id)
                            .onGeometryChange(for: CGRect.self) { geometry in
                                geometry.frame(in: .named("conversation-content"))
                            } action: { rect in store.trackFrame(message.id, chat: chatID, frame: rect) }
                            .onDisappear { store.trackFrame(message.id, chat: chatID, frame: nil) }
                        }
                        if !store.newerComplete {
                            Button(store.loadingNewer ? "Loading…" : "Load newer messages") {
                                historyAnchor = store.visibleMessageIDs.first; store.loadNewer()
                            }.disabled(store.loadingNewer || store.loading).font(.caption).padding(12)
                        }
                        Color.clear.frame(height: 1).id("conversation-bottom")
                    }.coordinateSpace(name: "conversation-content").padding(.horizontal, 12).padding(.bottom, 8)
                }
                .background(ChatAppearance.canvas)
                .accessibilityIdentifier("conversation-scroll")
                .scrollDismissesKeyboard(.interactively)
                .defaultScrollAnchor(.bottom, for: .initialOffset)
                .defaultScrollAnchor(.bottom, for: .alignment)
                .defaultScrollAnchor(.bottom, for: .sizeChanges)
                .onScrollGeometryChange(for: Viewport.self) { geometry in
                    Viewport(height: geometry.containerSize.height, nearBottom: geometry.contentSize.height - geometry.visibleRect.maxY < 100, rect: geometry.visibleRect)
                } action: { old, new in
                    if nearBottom != new.nearBottom { nearBottom = new.nearBottom }
                    store.trackViewport(new.rect, chat: chatID)
                    // Anchor after the resized viewport has been laid out. A
                    // composer panel can shrink it without changing any rows.
                    if old.height > 0 && old.height != new.height && old.nearBottom {
                        DispatchQueue.main.async { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                    }
                }
                .onChange(of: store.messages.last?.id) { _, _ in
                    guard !store.messages.isEmpty, historyAnchor == nil, store.newerComplete, store.restoredAnchor == nil, store.scrollTarget == nil else { return }
                    if !didInitialScroll || nearBottom || store.messages.last?.fromMe == true {
                        didInitialScroll = true
                        DispatchQueue.main.async { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                    }
                }
                .onChange(of: store.scrollTarget) { _, id in
                    if let id { didInitialScroll = true; nearBottom = false; withAnimation { proxy.scrollTo(id, anchor: .center) }; store.scrollTarget = nil }
                }
                .onScrollGeometryChange(for: Bool.self) { geometry in geometry.visibleRect.minY < 80 } action: { _, nearTop in
                    if nearTop && didInitialScroll && !nearBottom && !store.loading && !store.loadingNewer && !store.fetchingPhone && !(store.archiveComplete && store.phoneComplete) { historyAnchor = store.messages.first?.id; store.loadOlder() }
                }
                .onChange(of: pageBounds) { _, _ in
                    if let restored = store.restoredAnchor {
                        didInitialScroll = true; nearBottom = false; historyAnchor = nil
                        DispatchQueue.main.async { proxy.scrollTo(restored, anchor: .top) }
                        store.restoredAnchor = nil
                    } else if let historyAnchor {
                        DispatchQueue.main.async { proxy.scrollTo(historyAnchor, anchor: .top) }
                        self.historyAnchor = nil
                    }
                }
                .onChange(of: nearBottom) { _, atBottom in
                    if atBottom && !store.newerComplete && !store.loading && !store.loadingNewer && !store.fetchingPhone && !selecting {
                        historyAnchor = store.visibleMessageIDs.first; store.loadNewer()
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
                    if (!nearBottom || !store.newerComplete) && !store.messages.isEmpty {
                        Button {
                            if !store.newerComplete { didInitialScroll = false; store.loadLatest() }
                            else { withAnimation { proxy.scrollTo("conversation-bottom", anchor: .bottom) } }
                        } label: {
                            Image(systemName: "chevron.down").font(.system(size: 15, weight: .semibold)).foregroundStyle(.primary).frame(width: 44, height: 44)
                                .background(.regularMaterial, in: Circle())
                        }.padding(14).accessibilityLabel("Jump to latest message")
                    }
                }
                ChatComposer(chatID: chatID, draft: $draftText, audio: store.audio)
            }
        }
        .navigationTitle(chat.map(store.chatTitle) ?? "Chat")
        .toolbar {
            ToolbarItem(placement: .principal) {
                ConversationTitle(chatID: chatID) { infoPresented = true }
            }
            ToolbarItem(placement: .topBarTrailing) {
                if selecting {
                    Menu {
                        Button("Copy selected messages") {
                            UIPasteboard.general.string = MessageText.transcript(store.messages.filter { selection.contains($0.id) }, name: { store.displayName($0.sender, fallback: $0.senderName) }, mentions: store.mentionNames)
                            selecting = false; selection = []
                        }.disabled(selection.isEmpty)
                        Button("Cancel selection", role: .cancel) { selecting = false; selection = [] }
                    } label: { Image(systemName: "checkmark.circle.fill") }
                } else {
                    Button { infoPresented = true } label: {
                        AvatarView(name: chat.map(store.chatTitle) ?? "Chat", url: store.localURL(store.avatars[store.canonical(chatID)]), group: chat?.kind == "group", size: 32)
                    }.accessibilityLabel("View profile")
                }
            }
        }
        .sheet(isPresented: $infoPresented) { NavigationStack { ChatDetails(id: chatID).toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { infoPresented = false } } } } }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .quickLookPreview($previewURL)
        .onAppear {
            draftText = store.drafts[store.canonical(chatID), default: ""]
            store.open(chatID)
        }
        .onChange(of: selecting) { _, value in
            store.selectionActive = value
            if !value { store.trimConversation(towardOlder: false) }
        }
        .onDisappear {
            store.drafts[store.canonical(chatID)] = store.draftBeforeEditing ?? draftText
            store.close(chatID)
        }
        .onChange(of: phase) { _, phase in
            if phase != .active { store.drafts[store.canonical(chatID)] = store.draftBeforeEditing ?? draftText }
        }
    }

    private func joinsPrevious(_ index: Int) -> Bool {
        index > 0 && MessageGrouping.joins(store.messages[index - 1], store.messages[index])
    }

    private func joinsNext(_ index: Int) -> Bool {
        index + 1 < store.messages.count && MessageGrouping.joins(store.messages[index], store.messages[index + 1])
    }

    private func startsDay(_ index: Int) -> Bool {
        guard index > 0, store.messages.indices.contains(index) else { return true }
        return !Calendar.current.isDate(Date(timeIntervalSince1970: store.messages[index - 1].timestamp),
                                       inSameDayAs: Date(timeIntervalSince1970: store.messages[index].timestamp))
    }


}

/// Presence changes invalidate this small title, not the conversation transcript.
private struct ConversationTitle: View {
    let chatID: String
    let showInfo: () -> Void
    @Environment(ChatStore.self) private var store
    private var chat: Chat? { store.chats.first { $0.id == store.canonical(chatID) } }
    var body: some View {
        Button(action: showInfo) {
            VStack(spacing: 2) {
                Text(chat.map(store.chatTitle) ?? "Chat").font(.headline).foregroundStyle(Color.primary)
                if let chat, let label = store.presenceLabel(chat) {
                    Text(label).font(.caption2).foregroundStyle(Color.secondary).lineLimit(1)
                }
            }.lineLimit(1)
        }.accessibilityLabel("Chat information")
    }
}
