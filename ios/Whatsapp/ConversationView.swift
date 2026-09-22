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
    @State private var infoPresented = false
    @State private var selection = Set<String>()
    @State private var selecting = false

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
                        MessageBubble(message: message, group: chat?.kind == "group", selecting: selecting, selected: selection.contains(message.id), select: { selecting = true; if !selection.insert(message.id).inserted { selection.remove(message.id) } }) { message in
                            if let url = store.localURL(message.mediaPath) { previewURL = url }
                            else { store.download(message) }
                        }
                        .id(message.id)
                    }
                    Color.clear.frame(height: 1).id("conversation-bottom")
                }.padding(.horizontal, 12).padding(.bottom, 8)
            }
            .background(Color(.systemGroupedBackground))
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
            .onChange(of: store.scrollTarget) { _, id in
                if let id { withAnimation { proxy.scrollTo(id, anchor: .center) }; store.scrollTarget = nil }
            }
            .onScrollGeometryChange(for: Bool.self) { geometry in geometry.visibleRect.minY < 80 } action: { _, nearTop in
                if nearTop && didInitialScroll && !nearBottom && !store.loading && !store.fetchingPhone && !(store.archiveComplete && store.phoneComplete) { historyAnchor = store.messages.first?.id; store.loadOlder() }
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
            .safeAreaInset(edge: .bottom, spacing: 0) { ChatComposer(chatID: chatID, draft: $draftText, audio: store.audio) }
        }
        .navigationTitle(chat.map(store.chatTitle) ?? "Chat")
        .toolbar {
            ToolbarItem(placement: .principal) {
                Button { infoPresented = true } label: {
                    VStack {
                        Text(chat.map(store.chatTitle) ?? "Chat").font(.headline).foregroundStyle(Color.primary)
                        if let chat, let label = store.presenceLabel(chat) { Text(label).font(.caption2).foregroundStyle(Color.secondary).lineLimit(1) }
                    }
                }.accessibilityLabel("Chat information")
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
                } else { Button { infoPresented = true } label: { Image(systemName: "info.circle") } }
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
        .onDisappear {
            store.drafts[store.canonical(chatID)] = store.draftBeforeEditing ?? draftText
            store.close(chatID)
        }
        .onChange(of: phase) { _, phase in
            if phase != .active { store.drafts[store.canonical(chatID)] = store.draftBeforeEditing ?? draftText }
        }
    }

    private func startsDay(_ index: Int) -> Bool {
        guard index > 0, store.messages.indices.contains(index) else { return true }
        return !Calendar.current.isDate(Date(timeIntervalSince1970: store.messages[index - 1].timestamp),
                                       inSameDayAs: Date(timeIntervalSince1970: store.messages[index].timestamp))
    }


}
