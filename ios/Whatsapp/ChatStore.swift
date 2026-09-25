import SwiftUI
import Observation

@MainActor
@Observable
final class ChatStore {
    var chats: [Chat] = []
    var messages: [Message] = []
    var selectedChat: String?
    var status = "starting"
    var hasSession: Bool
    var qr: String?
    var pairingCode: String?
    var pairingBusy = false
    var pairingInterrupted = false
    var loading = false
    var fetchingPhone = false
    var archiveComplete = false
    var phoneComplete = false
    var syncProgress: Int?
    var error: String?
    var avatars: [String: String] = [:]
    var contactNames: [String: String] = [:]
    var aliases: [String: String] = [:]
    var drafts: [String: String] = [:]
    var reply: Message?
    var accountName = "Your account"
    var accountID: String?
    var accountAbout: String?
    var navigation: [String] = []
    var selectedTab = "chats"
    var preferences: Preferences { didSet {
        if !isDemo, let data = try? JSONEncoder().encode(preferences) { defaults.set(data, forKey: "preferences") }
    } }
    var searchQuery = ""
    var searchHits: [Message] = []
    var searching = false
    var editing: Message?
    var attachments: [String: [PendingAttachment]] = [:]
    var typing: [String: [String: Date]] = [:]
    var presence: [String: (online: Bool, lastSeen: TimeInterval?)] = [:]
    var fullAvatars: [String: String] = [:]
    var contacts: [CoreEvent.Contact] = []
    var scrollTarget: String?
    var newerComplete = true
    var loadingNewer = false
    var restoredAnchor: String?
    @ObservationIgnored var visibleMessageIDs: [String] = []
    @ObservationIgnored var visibleMessageFrames: [String: CGRect] = [:]
    @ObservationIgnored var messageViewport = CGRect.zero
    @ObservationIgnored var selectionActive = false
    @ObservationIgnored var loadLatestAfterPage = false
    @ObservationIgnored var recentConversations = ConversationCache()
    @ObservationIgnored var phoneHistory: [String: (complete: Bool, retryAfter: Date)] = [:]
    var infoChatID: String?
    var gifQuery = ""
    var gifs: [GifItem] = []
    var gifError: String?
    var gifsLoading = false
    var savedStickers: [String] = []
    var recentStickers: [String] = []
    var stickerPacks: [StickerPack] = []
    var newContactBusy = false
    var voiceSending = false
    let audio = NativeAudio()
    let notifications = LocalNotifications()
    @ObservationIgnored var pendingJump: String?
    @ObservationIgnored var pendingJumpChat: String?
    @ObservationIgnored var jumpRequested = false
    @ObservationIgnored var typingExpiry: DispatchWorkItem?
    @ObservationIgnored var audioRequest = UUID()
    @ObservationIgnored var playedMessages: Set<String> = []
    @ObservationIgnored var draftBeforeEditing: String?
    @ObservationIgnored var demoConversations: [String: [Message]] = [:]
    @ObservationIgnored var composingWork: DispatchWorkItem?
    @ObservationIgnored var lastComposing = Date.distantPast
    var sendReadReceipts: Bool {
        didSet { if !isDemo { defaults.set(sendReadReceipts, forKey: "readReceipts") } }
    }

    let isDemo: Bool
    private let defaults: UserDefaults
    let engine: MessagingEngine
    private let backgroundActivity: BackgroundActivityManaging
    private let diagnostics: ConnectionDiagnostics
    @ObservationIgnored private var root: URL?
    @ObservationIgnored private var observer: NSObjectProtocol?
    @ObservationIgnored private var engineStarted = false
    @ObservationIgnored var foreground = false
    @ObservationIgnored private var receiptsDisabled = false
    @ObservationIgnored private var requestedAvatars: Set<String> = []
    @ObservationIgnored var phoneRetryAfter = Date.distantPast
    @ObservationIgnored private var suspendWork: DispatchWorkItem?
    @ObservationIgnored private var backgroundTask = UIBackgroundTaskIdentifier.invalid
    @ObservationIgnored private var backgroundGeneration = 0
    @ObservationIgnored private var stoppingBackgroundTasks: [Int: BackgroundActivityCompletion] = [:]

    var connected: Bool { status == "connected" }
    var canPost: Bool { (connected || isDemo) && selectedChat != nil && currentChat?.readOnly != true }
    var receiptsAllowed: Bool { receiptsAllowed(in: selectedChat ?? "") }
    func receiptsAllowed(in chat: String) -> Bool { sendReadReceipts && (chat.hasSuffix("@g.us") || !receiptsDisabled) }
    var currentChat: Chat? { chats.first { $0.id == selectedChat } }
    var connectionLabel: String {
        if isDemo { return "Offline preview" }
        if let syncProgress { return "Syncing history · \(syncProgress)%" }
        switch status {
        case "connected": return "Connected"
        case "suspended": return "Connection paused"
        case "disconnected", "failed": return "Waiting to reconnect"
        case "unlinked", "logged_out": return "Link your account"
        default: return "Connecting…"
        }
    }

    init(demo: Bool = false, defaults: UserDefaults = .standard,
         engine: MessagingEngine = CoreEngine(), backgroundActivity: BackgroundActivityManaging? = nil) {
        self.isDemo = demo
        self.defaults = defaults
        self.engine = engine
        self.backgroundActivity = backgroundActivity ?? BackgroundActivity()
        self.diagnostics = ConnectionDiagnostics(defaults: defaults)
        self.preferences = defaults.data(forKey: "preferences").flatMap { try? JSONDecoder().decode(Preferences.self, from: $0) } ?? Preferences()
        self.hasSession = demo || defaults.bool(forKey: "hasLinkedSession")
        self.sendReadReceipts = defaults.object(forKey: "readReceipts") as? Bool ?? true
        if demo { root = Self.demoRoot; loadDemo(); return }
        notifications.onOpen = { [weak self] id in self?.navigate(to: id) }
        diagnostics.record(.appStarted)
        engine.onEvents = { [weak self] in self?.apply($0) }
        engine.onError = { [weak self] in
            self?.engineStarted = false
            self?.diagnostics.record(.engineError)
            self?.error = $0; self?.status = "failed"
            self?.loading = false; self?.fetchingPhone = false
        }
        observer = NotificationCenter.default.addObserver(forName: .coreEventsReady, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.engine.drain() }
        }
    }

    func activate() {
        foreground = true
        backgroundGeneration += 1
        suspendWork?.cancel()
        suspendWork = nil
        finishBackgroundTask()
        guard !isDemo else { return }
        diagnostics.record(.foreground)
        // Inactive scenes and short app switches leave the worker running.
        // It already archives and delivers updates; reloading here repeats work
        // and flashes a spinner over a conversation that is still current.
        guard !engineStarted else { return }
        do {
            if root == nil { root = try CoreEngine.storageDirectory() }
            guard let root else { return }
            if status == "suspended" || status == "suspending" { status = "connecting" }
            engineStarted = true
            recentConversations.removeAll()
            requestedAvatars.removeAll()
            engine.start(root: root)
            if let selectedChat {
                loading = true
                if !newerComplete, let anchor = visibleMessageIDs.first ?? messages.first?.id {
                    restoredAnchor = anchor
                    engine.send(["type": "around", "chat": selectedChat, "id": anchor])
                } else { engine.send(["type": "load", "chat": selectedChat]) }
            }
        } catch { self.error = "Could not create private storage on this iPhone." }
    }

    func prepareForBackground() {
        guard !isDemo, backgroundTask == .invalid else { return }
        backgroundGeneration += 1
        let generation = backgroundGeneration
        // Acquire the assertion while still in the foreground, before pairing
        // or the scene's inactive -> background transition. Starting it only
        // after backgrounding does not establish a reliable UIKit allowance.
        backgroundTask = backgroundActivity.begin { [weak self] in
            Task { @MainActor in self?.expireBackgroundActivity(generation: generation) }
        }
        diagnostics.record(backgroundTask == .invalid ? .backgroundTaskUnavailable : .backgroundTaskStarted)
    }

    func background() {
        foreground = false
        audioRequest = UUID()
        audio.pauseForBackground()
        stopComposing()
        guard !isDemo else { return }
        // Fallback for a scene without a preceding inactive notification.
        // An unavailable assertion stops immediately rather than trusting a timer.
        if backgroundTask == .invalid { prepareForBackground() }
        diagnostics.record(.background, remaining: backgroundActivity.timeRemaining)
        checkBackgroundBudget(generation: backgroundGeneration)
    }

    private func checkBackgroundBudget(generation: Int) {
        guard generation == backgroundGeneration, !foreground else { return }
        suspendWork?.cancel()
        guard backgroundTask != .invalid,
              let delay = BackgroundActivity.nextCheck(remaining: backgroundActivity.timeRemaining) else {
            suspend(generation: generation)
            return
        }
        let work = DispatchWorkItem { [weak self] in self?.checkBackgroundBudget(generation: generation) }
        suspendWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func suspend(generation: Int, expired: Bool = false) {
        guard generation == backgroundGeneration else { return }
        guard !foreground else {
            if expired { finishBackgroundTask() }
            return
        }
        suspendWork?.cancel()
        suspendWork = nil
        let task = backgroundTask
        backgroundTask = .invalid
        diagnostics.record(expired ? .backgroundExpired : .backgroundBudgetLow, remaining: backgroundActivity.timeRemaining)
        if !hasSession && (pairingCode != nil || pairingBusy) { pairingInterrupted = true }
        pairingCode = nil
        qr = nil
        pairingBusy = false
        status = "suspending"
        engineStarted = false
        // An unexpected expiration cannot wait for asynchronous shutdown.
        // The normal budget check starts shutdown before this deadline.
        let completion = BackgroundActivityCompletion(activity: backgroundActivity, task: task)
        stoppingBackgroundTasks[generation] = completion
        if expired { completion.end() }
        engine.stop { [weak self] in
            completion.end()
            self?.stoppingBackgroundTasks.removeValue(forKey: generation)
            self?.diagnostics.record(.engineStopped)
            if let self, !self.foreground, generation == self.backgroundGeneration {
                self.status = "suspended"; self.loading = false; self.fetchingPhone = false
                self.pairingCode = nil; self.qr = nil; self.pairingBusy = false
            }
        }
    }

    private func expireBackgroundActivity(generation: Int) {
        if let completion = stoppingBackgroundTasks[generation] {
            // UIKit can revoke the reserved time while Rust is already stopping.
            // End only that assertion, without stopping a newer connection.
            diagnostics.record(.backgroundExpired)
            completion.end()
        } else {
            suspend(generation: generation, expired: true)
        }
    }

    private func finishBackgroundTask() {
        if backgroundTask != .invalid {
            backgroundActivity.end(backgroundTask)
            backgroundTask = .invalid
        }
    }

    func canonical(_ chat: String) -> String {
        var result = chat
        var seen: Set<String> = []
        while let next = aliases[result], seen.insert(result).inserted { result = next }
        return result
    }

    func open(_ chat: String) {
        let chat = canonical(chat)
        guard selectedChat != chat else { return }
        stopComposing()
        audioRequest = UUID()
        if let current = selectedChat { rememberConversation(current) }
        selectedChat = chat
        if pendingJumpChat != chat { pendingJump = nil; pendingJumpChat = nil }
        messages = []
        reply = nil
        editing = nil
        archiveComplete = false
        phoneComplete = phoneHistory[chat]?.complete ?? false
        phoneRetryAfter = phoneHistory[chat]?.retryAfter ?? .distantPast
        fetchingPhone = false
        loadingNewer = false; newerComplete = true; restoredAnchor = nil
        visibleMessageIDs = []; visibleMessageFrames = [:]; messageViewport = .zero; selectionActive = false; loadLatestAfterPage = false
        if isDemo { messages = demoConversations[chat] ?? Self.demoMessages(chat: chat); archiveComplete = true; phoneComplete = true; loading = false; resolveJump(); return }
        if let page = recentConversations.take(chat) {
            messages = page.messages; archiveComplete = page.archiveComplete
            phoneComplete = page.phoneComplete; phoneRetryAfter = page.phoneRetryAfter
            newerComplete = page.newerComplete; restoredAnchor = page.anchor
            loading = false
            resolveJump()
        } else {
            loading = true
            engine.send(["type": "load", "chat": chat])
        }
        markRead()
    }

    func close(_ chat: String) {
        guard selectedChat == canonical(chat) else { return }
        if isDemo { demoConversations[canonical(chat)] = messages }
        else { rememberConversation(canonical(chat)) }
        selectedChat = nil
        loadLatestAfterPage = false
        audioRequest = UUID()
        stopComposing(chat: canonical(chat))
        audio.pauseForBackground()
        audio.stopPlayback()
        messages = []
        reply = nil
        editing = nil
        draftBeforeEditing = nil
        loading = false
        fetchingPhone = false
    }

    func loadOlder() {
        guard let selectedChat, !loading, !loadingNewer, !fetchingPhone, !isDemo else { return }
        if !archiveComplete, let first = messages.first {
            loading = true
            engine.send(["type": "load", "chat": selectedChat, "before": [Int64(first.timestamp), first.id]])
        } else if !archiveComplete {
            loading = true
            engine.send(["type": "load", "chat": selectedChat])
        } else if connected && !phoneComplete && Date() >= phoneRetryAfter {
            fetchingPhone = true
            phoneRetryAfter = Date().addingTimeInterval(30)
            engine.send(["type": "older", "chat": selectedChat])
        }
    }

    func pair(phone: String) {
        let digits = phone.filter(\.isASCII).filter(\.isNumber)
        guard (7...15).contains(digits.count) else { error = "Enter your phone number with its country code."; return }
        prepareForBackground()
        if !isDemo { diagnostics.record(.pairingRequested) }
        pairingInterrupted = false
        pairingBusy = true
        engine.send(["type": "pair", "phone": digits]) { [weak self] accepted in
            if !accepted { self?.pairingBusy = false; self?.error = "The connection is not ready. Try again in a moment." }
        }
    }

    func reconnect() {
        guard !isDemo else { return }
        status = "connecting"
        engine.send(["type": "reconnect"])
    }

    func sendText(_ text: String, completion: @escaping (Bool) -> Void) {
        guard let chat = selectedChat, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              canPost else { completion(false); return }
        guard text.unicodeScalars.count <= 65_536 else {
            error = "This message is too long. Split it into smaller messages."
            completion(false)
            return
        }
        if let editing {
            submitEdit(editing, text: text, completion: completion)
            return
        }
        drafts[chat] = text
        if isDemo {
            messages.append(Self.sampleMessage(id: UUID().uuidString, chat: chat, text: text, fromMe: true, time: Date().timeIntervalSince1970, status: "pending"))
            drafts[chat] = ""
            reply = nil
            completion(true)
            return
        }
        var command: [String: Any] = ["type": "send", "chat": chat, "text": text, "mentions": mentionedIDs(in: text)]
        if let reply { command["quoting"] = reply.id }
        let quotedID = reply?.id
        engine.send(command) { [weak self] accepted in
            guard let self else { return }
            if accepted {
                if self.drafts[chat] == text { self.drafts[chat] = "" }
                if self.reply?.id == quotedID { self.reply = nil }
            } else { self.error = "The message was not queued. Your draft is still here." }
            completion(accepted)
        }
    }

    func avatar(_ id: String) {
        guard !isDemo, requestedAvatars.insert(id).inserted else { return }
        engine.send(["type": "avatar", "id": id])
    }

    func download(_ message: Message) {
        guard !isDemo, connected, message.mediaState != "downloading" else { return }
        if let index = messages.firstIndex(where: { $0.id == message.id }) { messages[index].mediaState = "downloading" }
        engine.send(["type": "download", "chat": message.chat, "message": message.id])
    }

    func localURL(_ path: String?) -> URL? {
        guard let path, let root else { return nil }
        let url = URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath()
        let prefix = root.standardizedFileURL.resolvingSymlinksInPath().path + "/"
        return url.path.hasPrefix(prefix) ? url : nil
    }

    private func markRead() {
        guard foreground, let selectedChat, !isDemo else { return }
        let allowed = sendReadReceipts && (currentChat?.kind == "group" || !receiptsDisabled)
        engine.send(["type": "read", "chat": selectedChat, "receipts": allowed])
    }

    func apply(_ events: [CoreEvent]) {
        for event in CoreEvent.coalescing(events) {
            // The worker archives all changes. Drop an inactive snapshot when its
            // contents change so a later open cannot revive deleted/edited data.
            if ["messages", "message", "deleted", "media", "older", "newer", "around"].contains(event.type),
               let id = (event.chat ?? event.message?.chat).map(canonical), id != selectedChat {
                recentConversations.remove(id)
            }
            switch event.type {
            case "link":
                // A code queued before shutdown no longer belongs to a live
                // connection. Do not put it back on the pairing screen.
                if (status == "suspending" || status == "suspended") && event.status == "unlinked" { continue }
                status = event.status ?? "failed"
                if !isDemo {
                    diagnostics.record(ConnectionDiagnostics.Stage(rawValue: status) ?? .unknownLinkState)
                    if event.code != nil { diagnostics.record(.codeReady) }
                }
                qr = event.qr
                pairingCode = event.code
                if event.code != nil { pairingInterrupted = false }
                if event.code != nil || status != "unlinked" { pairingBusy = false }
                if status == "connected" {
                    pairingInterrupted = false
                    if foreground { finishBackgroundTask() }
                    hasSession = true
                    defaults.set(true, forKey: "hasLinkedSession")
                    requestedAvatars = []
                    markRead()
                } else if status == "unlinked" || status == "logged_out" {
                    clearMemoryCaches()
                    phoneHistory.removeAll()
                    hasSession = false
                    defaults.set(false, forKey: "hasLinkedSession")
                    chats = []; messages = []; selectedChat = nil; drafts = [:]; reply = nil
                    if status == "logged_out" {
                        navigation = []; editing = nil; attachments = [:]; audio.discardRecording(); audio.stopPlayback()
                        contacts = []; contactNames = [:]; avatars = [:]; fullAvatars = [:]; aliases = [:]
                        searchHits = []; searchQuery = ""; searching = false
                        typing = [:]; typingExpiry?.cancel(); presence = [:]
                        pendingJump = nil; pendingJumpChat = nil; scrollTarget = nil; draftBeforeEditing = nil
                        audioRequest = UUID(); playedMessages = []
                        accountID = nil; accountName = "Your account"; accountAbout = nil
                        gifs = []; gifQuery = ""; savedStickers = []; recentStickers = []; stickerPacks = []
                        if !isDemo, let root { engine.clearTransientMedia(root: root); notifications.clear() }
                    }
                }
                if status == "failed" { error = event.detail ?? "Could not connect to WhatsApp." }
            case "me": accountName = event.name ?? "Your account"; accountID = event.id; accountAbout = event.about
            case "chats":
                chats = OrderedUpdates.merge(chats, event.chats ?? []) { $0.pinned != $1.pinned ? $0.pinned : ($0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp > $1.timestamp) }
            case "messages":
                guard event.chat.map(canonical) == selectedChat else { continue }
                let incoming = event.messages ?? []
                if !newerComplete, event.requested != true,
                   incoming.contains(where: { $0.fromMe && $0.status == "pending" }) {
                    loadLatest(); continue
                }
                // Live messages beyond an older window are already in SQLite;
                // do not join two non-contiguous ranges and skip the gap.
                let accepted = !newerComplete && event.requested != true
                    ? incoming.filter { update in messages.contains { $0.id == update.id } } : incoming
                messages = ConversationMessages.merge(messages, accepted.map { message in
                    var message = message; message.chat = canonical(message.chat); return message
                })
                if event.requested == true {
                    loading = false
                    archiveComplete = event.complete ?? false
                    if messages.isEmpty && archiveComplete { loadOlder() }
                    resolveJump()
                }
                trimConversation(towardOlder: event.older == true)
                markRead()
            case "around":
                guard event.chat.map(canonical) == selectedChat else { continue }
                messages = event.messages ?? []; loading = false; loadingNewer = false
                archiveComplete = event.complete ?? false; newerComplete = event.more != true
                resolveJump()
            case "newer":
                guard event.chat.map(canonical) == selectedChat, loadingNewer else { continue }
                messages = ConversationMessages.merge(messages, event.messages ?? [])
                newerComplete = event.complete ?? false; loadingNewer = false
                trimConversation(towardOlder: false)
            case "message":
                if var message = event.message, canonical(message.chat) == selectedChat,
                   let index = messages.firstIndex(where: { $0.id == message.id }) {
                    message.chat = canonical(message.chat)
                    if messages[index] != message {
                        if messages[index].timestamp == message.timestamp { messages[index] = message }
                        else { messages = ConversationMessages.merge(messages, [message]) }
                    }
                }
            case "load_failed":
                if event.chat.map(canonical) == selectedChat {
                    loading = false; loadingNewer = false
                    pendingJump = nil; pendingJumpChat = nil; jumpRequested = false
                    error = event.detail ?? "Could not load this chat."
                }
            case "merged":
                guard let from = event.from, let into = event.into else { continue }
                recentConversations.remove(from); recentConversations.remove(into)
                phoneHistory.removeValue(forKey: from); phoneHistory.removeValue(forKey: into)
                aliases[from] = into
                chats.removeAll { $0.id == from }
                if let draft = drafts.removeValue(forKey: from), drafts[into, default: ""].isEmpty { drafts[into] = draft }
                if let pending = attachments.removeValue(forKey: from) { attachments[into, default: []].append(contentsOf: pending) }
                navigation = navigation.map { $0 == from ? into : $0 }
                if pendingJumpChat == from { pendingJumpChat = into }
                if selectedChat == from {
                    selectedChat = into
                    messages = messages.map { var message = $0; message.chat = into; return message }
                    if var quote = reply { quote.chat = into; reply = quote }
                    if var edited = editing { edited.chat = into; editing = edited }
                    loading = true
                    engine.send(["type": "load", "chat": into])
                }
            case "deleted":
                if event.chat.map(canonical) == selectedChat { messages.removeAll { $0.id == event.id } }
            case "contacts":
                var byID = Dictionary(contacts.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
                for contact in event.contacts ?? [] { byID[contact.id] = contact }
                contacts = Array(byID.values)
                for contact in event.contacts ?? [] { if let name = contact.name { contactNames[contact.id] = name } }
            case "avatar":
                if let id = event.id {
                    if event.full == true { fullAvatars[id] = event.path } else { avatars[id] = event.path }
                }
            case "search":
                if event.query == searchQuery { searchHits = event.messages ?? []; searching = false }
            case "typing":
                if let chat = event.chat.map(canonical), let id = event.id {
                    typing[chat, default: [:]][id] = event.composing == true ? Date().addingTimeInterval(12) : nil
                    scheduleTypingExpiry()
                }
            case "presence":
                if let id = event.id { presence[canonical(id)] = (event.online == true, event.lastSeen) }
            case "contact_ready":
                newContactBusy = false
                if let id = event.id {
                    engine.send(["type": "ensure", "chat": id, "name": event.name ?? displayName(id)])
                    navigate(to: id)
                }
            case "gifs":
                if event.query == gifQuery { gifs = event.gifs ?? []; gifError = event.detail; gifsLoading = false }
            case "stickers":
                savedStickers = event.saved ?? []; recentStickers = event.recent ?? []; stickerPacks = event.packs ?? []
            case "incoming":
                if let message = event.message, preferences.notifications, !isDemo,
                   (!foreground || selectedChat != canonical(message.chat)),
                   chats.first(where: { $0.id == canonical(message.chat) })?.muted != true {
                    notifications.show(message, name: chats.first(where: { $0.id == canonical(message.chat) })?.name ?? displayName(message.sender))
                }
            case "media":
                if event.chat.map(canonical) == selectedChat, let index = messages.firstIndex(where: { $0.id == event.id }) {
                    messages[index].mediaPath = event.path
                    messages[index].mediaState = event.path == nil ? "failed" : "idle"
                    messages[index].mediaError = event.detail
                }
            case "sync":
                syncProgress = event.active == true ? 0 : nil
                if !isDemo { diagnostics.record(event.active == true ? .syncStarted : .syncFinished) }
            case "progress": syncProgress = event.progress
            case "older":
                if let chat = event.chat.map(canonical) {
                    phoneHistory[chat] = (event.more == false, phoneHistory[chat]?.retryAfter ?? .distantPast)
                }
                if event.chat.map(canonical) == selectedChat {
                    fetchingPhone = false
                    phoneComplete = event.more == false
                    // A chunk can archive more than the worker's 500-row reply,
                    // or arrive after its request timed out. Page SQLite again.
                    archiveComplete = false
                }
            case "privacy": receiptsDisabled = event.disabled ?? true
            case "error", "info":
                error = event.detail; pairingBusy = false
                newContactBusy = false
                if !isDemo && event.type == "error" { diagnostics.record(.updateError) }
            default: break
            }
            if loadLatestAfterPage && !loading && !loadingNewer { loadLatest() }
        }
    }
}
