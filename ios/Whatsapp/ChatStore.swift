import SwiftUI

@MainActor
final class ChatStore: ObservableObject {
    @Published var chats: [Chat] = []
    @Published var messages: [Message] = []
    @Published var selectedChat: String?
    @Published var status = "starting"
    @Published var hasSession: Bool
    @Published var qr: String?
    @Published var pairingCode: String?
    @Published var pairingBusy = false
    @Published var loading = false
    @Published var fetchingPhone = false
    @Published var archiveComplete = false
    @Published var phoneComplete = false
    @Published var syncProgress: Int?
    @Published var error: String?
    @Published var avatars: [String: String] = [:]
    @Published var contactNames: [String: String] = [:]
    @Published var aliases: [String: String] = [:]
    @Published var drafts: [String: String] = [:]
    @Published var reply: Message?
    @Published var accountName = "Your account"
    @Published var sendReadReceipts: Bool {
        didSet { if !isDemo { defaults.set(sendReadReceipts, forKey: "readReceipts") } }
    }

    let isDemo: Bool
    private let defaults: UserDefaults
    private let engine = CoreEngine()
    private var root: URL?
    private var observer: NSObjectProtocol?
    private var foreground = false
    private var receiptsDisabled = false
    private var requestedAvatars: Set<String> = []
    private var phoneRetryAfter = Date.distantPast
    private var suspendWork: DispatchWorkItem?
    private var backgroundTask = UIBackgroundTaskIdentifier.invalid
    private var backgroundGeneration = 0

    var connected: Bool { status == "connected" }
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

    init(demo: Bool = false, defaults: UserDefaults = .standard) {
        self.isDemo = demo
        self.defaults = defaults
        self.hasSession = demo || defaults.bool(forKey: "hasLinkedSession")
        self.sendReadReceipts = defaults.object(forKey: "readReceipts") as? Bool ?? true
        if demo { loadDemo(); return }
        engine.onEvents = { [weak self] in self?.apply($0) }
        engine.onError = { [weak self] in
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
        do {
            root = try CoreEngine.storageDirectory()
            guard let root else { return }
            if status == "suspended" { status = "connecting" }
            engine.start(root: root)
            if let selectedChat {
                loading = true
                engine.send(["type": "load", "chat": selectedChat])
            }
        } catch { self.error = "Could not create private storage on this iPhone." }
    }

    func background() {
        foreground = false
        guard !isDemo, backgroundTask == .invalid else { return }
        backgroundGeneration += 1
        let generation = backgroundGeneration
        // Finish current pairing/sends, then close the socket before suspension.
        // No background mode or silent keepalive is requested.
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Finish WhatsApp activity") { [weak self] in
            Task { @MainActor in self?.suspend(generation: generation) }
        }
        let work = DispatchWorkItem { [weak self] in self?.suspend(generation: generation) }
        suspendWork = work
        let remaining = UIApplication.shared.backgroundTimeRemaining
        let grace = backgroundTask == .invalid ? 0 : max(0, min(20, remaining - 9))
        DispatchQueue.main.asyncAfter(deadline: .now() + grace, execute: work)
    }

    private func suspend(generation: Int) {
        guard generation == backgroundGeneration, !foreground else { return }
        suspendWork?.cancel()
        suspendWork = nil
        let task = backgroundTask
        backgroundTask = .invalid
        engine.stop { [weak self] in
            if let self, !self.foreground, generation == self.backgroundGeneration {
                self.status = "suspended"; self.loading = false; self.fetchingPhone = false
            }
            if task != .invalid { UIApplication.shared.endBackgroundTask(task) }
        }
    }

    private func finishBackgroundTask() {
        if backgroundTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundTask)
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
        selectedChat = chat
        messages = []
        reply = nil
        archiveComplete = false
        phoneComplete = false
        phoneRetryAfter = .distantPast
        fetchingPhone = false
        if isDemo { messages = Self.demoMessages(chat: chat); archiveComplete = true; phoneComplete = true; return }
        loading = true
        engine.send(["type": "load", "chat": chat])
        markRead()
    }

    func close(_ chat: String) {
        guard selectedChat == canonical(chat) else { return }
        selectedChat = nil
        messages = []
        reply = nil
        loading = false
        fetchingPhone = false
    }

    func loadOlder() {
        guard let selectedChat, !loading, !fetchingPhone, !isDemo else { return }
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
              connected || isDemo else { completion(false); return }
        guard text.unicodeScalars.count <= 65_536 else {
            error = "This message is too long. Split it into smaller messages."
            completion(false)
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
        var command: [String: Any] = ["type": "send", "chat": chat, "text": text]
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
        guard !isDemo, connected, requestedAvatars.insert(id).inserted else { return }
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
        for event in events {
            switch event.type {
            case "link":
                status = event.status ?? "failed"
                qr = event.qr
                pairingCode = event.code
                if event.code != nil || status != "unlinked" { pairingBusy = false }
                if status == "connected" {
                    hasSession = true
                    defaults.set(true, forKey: "hasLinkedSession")
                    requestedAvatars = []
                    markRead()
                } else if status == "unlinked" || status == "logged_out" {
                    hasSession = false
                    defaults.set(false, forKey: "hasLinkedSession")
                    chats = []; messages = []; selectedChat = nil; drafts = [:]; reply = nil
                }
                if status == "failed" { error = event.detail ?? "Could not connect to WhatsApp." }
            case "me": accountName = event.name ?? "Your account"
            case "chats":
                var byID = Dictionary(chats.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
                for chat in event.chats ?? [] { byID[chat.id] = chat }
                chats = byID.values.sorted { $0.pinned != $1.pinned ? $0.pinned : ($0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp > $1.timestamp) }
            case "messages":
                guard event.chat.map(canonical) == selectedChat else { continue }
                messages = ConversationMessages.merge(messages, (event.messages ?? []).map { message in
                    var message = message; message.chat = canonical(message.chat); return message
                })
                if event.requested == true {
                    loading = false
                    archiveComplete = event.complete ?? false
                    if messages.isEmpty && archiveComplete { loadOlder() }
                }
                markRead()
            case "message":
                if var message = event.message, canonical(message.chat) == selectedChat,
                   let index = messages.firstIndex(where: { $0.id == message.id }) {
                    message.chat = canonical(message.chat)
                    messages[index] = message
                }
            case "load_failed":
                if event.chat.map(canonical) == selectedChat { loading = false; error = event.detail ?? "Could not load this chat." }
            case "merged":
                guard let from = event.from, let into = event.into else { continue }
                aliases[from] = into
                chats.removeAll { $0.id == from }
                if let draft = drafts.removeValue(forKey: from), drafts[into, default: ""].isEmpty { drafts[into] = draft }
                if selectedChat == from {
                    selectedChat = into
                    messages = messages.map { var message = $0; message.chat = into; return message }
                    if var quote = reply { quote.chat = into; reply = quote }
                    loading = true
                    engine.send(["type": "load", "chat": into])
                }
            case "deleted":
                if event.chat.map(canonical) == selectedChat { messages.removeAll { $0.id == event.id } }
            case "contacts":
                for contact in event.contacts ?? [] { if let name = contact.name { contactNames[contact.id] = name } }
            case "avatar":
                if let id = event.id { avatars[id] = event.path }
            case "media":
                if event.chat.map(canonical) == selectedChat, let index = messages.firstIndex(where: { $0.id == event.id }) {
                    messages[index].mediaPath = event.path
                    messages[index].mediaState = event.path == nil ? "failed" : "idle"
                    messages[index].mediaError = event.detail
                }
            case "sync": syncProgress = event.active == true ? 0 : nil
            case "progress": syncProgress = event.progress
            case "older":
                if event.chat.map(canonical) == selectedChat {
                    fetchingPhone = false
                    phoneComplete = event.more == false
                    // A chunk can archive more than the worker's 500-row reply,
                    // or arrive after its request timed out. Page SQLite again.
                    archiveComplete = false
                }
            case "privacy": receiptsDisabled = event.disabled ?? true
            case "error", "info": error = event.detail; pairingBusy = false
            default: break
            }
        }
    }
}
