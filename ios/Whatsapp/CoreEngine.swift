import Foundation
import UIKit

extension Notification.Name {
    static let coreEventsReady = Notification.Name("WhatsappCoreEventsReady")
}

// The Rust worker can emit hundreds of history events together. Coalesce them
// before touching the main queue; the archive, decoding and networking stay off it.
private final class CoreWakeup: @unchecked Sendable {
    static let shared = CoreWakeup()
    private let lock = NSLock()
    private var queued = false

    func signal() {
        lock.lock()
        guard !queued else { lock.unlock(); return }
        queued = true
        lock.unlock()
        DispatchQueue.main.async {
            self.lock.lock()
            self.queued = false
            self.lock.unlock()
            NotificationCenter.default.post(name: .coreEventsReady, object: nil)
        }
    }
}

@_cdecl("whatsapp_events_ready")
func whatsappEventsReady() { CoreWakeup.shared.signal() }

/// Every handle operation, including shutdown, is serialized on this queue.
/// The callback only schedules a drain, so it never re-enters Rust's session lock.
protocol MessagingEngine: AnyObject {
    var onEvents: (([CoreEvent]) -> Void)? { get set }
    var onError: ((String) -> Void)? { get set }
    func start(root: URL)
    func drain()
    func send(_ command: [String: Any], completion: ((Bool) -> Void)?)
    func stop(completion: @escaping () -> Void)
    func prepareAudio(_ url: URL, completion: @escaping (URL?) -> Void)
    func clearTransientMedia(root: URL)
}

extension MessagingEngine {
    func send(_ command: [String: Any]) { send(command, completion: nil) }
    func prepareAudio(_ url: URL, completion: @escaping (URL?) -> Void) { completion(nil) }
    func clearTransientMedia(root: URL) {}
}

final class CoreEngine: MessagingEngine, @unchecked Sendable {
    private let queue = DispatchQueue(label: "org.erlin.whatsapp.ios.engine", qos: .userInitiated)
    private var handle: UInt64 = 0
    private var root: URL?
    private var audioRequests: [String: (URL?) -> Void] = [:]
    private var deferredPortraits: [String] = []
    private var portraitRestoreScheduled = false
    private var generation = 0
    var onEvents: (([CoreEvent]) -> Void)?
    var onError: ((String) -> Void)?

    static func storageDirectory() throws -> URL {
        var root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                               appropriateFor: nil, create: true).appendingPathComponent("Whatsapp", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try root.setResourceValues(values)
        return root
    }

    func start(root: URL) {
        queue.async {
            self.root = root
            if self.handle == 0 {
                self.handle = root.path.withCString { wa_start($0, whatsappEventsReady) }
            }
            guard self.handle != 0 else {
                self.fail("Could not open the iPhone's local message store. Close and reopen the app to try again.")
                return
            }
            self.drainOnQueue()
        }
    }

    func drain() { queue.async { self.drainOnQueue() } }

    private func drainOnQueue() {
        guard handle != 0, let pointer = wa_poll(handle) else { return }
        defer { wa_free_string(pointer) }
        let data = Data(bytes: pointer, count: strlen(pointer))
        do {
            let batch = try JSONDecoder().decode(CoreBatch.self, from: data)
            guard batch.version == 1 else { throw CocoaError(.coderInvalidValue) }
            guard !batch.events.isEmpty else { return }
            // Decode cached portraits alongside local chat/history restoration,
            // before rows appear. This never waits for the reconnecting worker
            // to service an avatar command and never performs UI-thread decoding.
            var normal: [CoreEvent] = []
            for event in batch.events {
                if event.type == "audio", let id = event.id {
                    if let callback = audioRequests.removeValue(forKey: id) {
                        let url = event.path.map { URL(fileURLWithPath: $0) }
                        DispatchQueue.main.async { callback(url) }
                    }
                } else { normal.append(event) }
            }
            let prepared = root.map { CachedMedia.plan(normal, root: $0) }
            let events = prepared?.events ?? normal
            DispatchQueue.main.async { self.onEvents?(events) }
            let explicit = Set(normal.filter { $0.type == "avatar" || $0.type == "merged" }.compactMap { $0.id ?? $0.from })
            deferredPortraits.removeAll { explicit.contains($0) }
            var known = Set(deferredPortraits)
            for id in prepared?.deferred ?? [] where known.insert(id).inserted { deferredPortraits.append(id) }
            schedulePortraitRestoration()
        } catch {
            // Never print the event JSON: pairing codes and private messages live here.
            fail("This build could not read a messaging update. Reopen the app to reconnect.")
        }
    }

    private func schedulePortraitRestoration() {
        guard !portraitRestoreScheduled, !deferredPortraits.isEmpty else { return }
        portraitRestoreScheduled = true
        let generation = generation
        queue.async {
            guard generation == self.generation else { return }
            self.portraitRestoreScheduled = false
            guard self.handle != 0, let root = self.root else { return }
            let ids = Array(self.deferredPortraits.prefix(CachedMedia.restoreLimit))
            self.deferredPortraits.removeFirst(ids.count)
            let events = CachedMedia.restore(ids, root: root)
            if !events.isEmpty { DispatchQueue.main.async { self.onEvents?(events) } }
            // Yield between slices so commands and connection events can run.
            self.schedulePortraitRestoration()
        }
    }

    func send(_ command: [String: Any], completion: ((Bool) -> Void)? = nil) {
        queue.async {
            let accepted: Bool
            if self.handle != 0, let data = try? JSONSerialization.data(withJSONObject: command),
               let json = String(data: data, encoding: .utf8) {
                accepted = json.withCString { wa_command(self.handle, $0) == 1 }
            } else { accepted = false }
            DispatchQueue.main.async { completion?(accepted) }
        }
    }

    func stop(completion: @escaping () -> Void) {
        queue.async {
            let handle = self.handle
            self.handle = 0
            self.generation += 1
            self.deferredPortraits.removeAll(); self.portraitRestoreScheduled = false
            let callbacks = Array(self.audioRequests.values)
            self.audioRequests.removeAll()
            DispatchQueue.main.async { callbacks.forEach { $0(nil) } }
            if handle != 0 { wa_stop(handle) }
            DispatchQueue.main.async(execute: completion)
        }
    }

    func prepareAudio(_ url: URL, completion: @escaping (URL?) -> Void) {
        queue.async {
            let request = UUID().uuidString
            let accepted = url.path.withCString { source in request.withCString { wa_prepare_audio(self.handle, source, $0) == 1 } }
            if accepted { self.audioRequests[request] = completion }
            else { DispatchQueue.main.async { completion(nil) } }
        }
    }

    func clearTransientMedia(root: URL) {
        queue.async { AttachmentImport.clearTransientMedia(root: root) }
    }

    private func fail(_ message: String) {
        DispatchQueue.main.async { self.onError?(message) }
    }
}
