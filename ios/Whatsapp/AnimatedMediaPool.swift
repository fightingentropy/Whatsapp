import UIKit

/// Reservations cover in-flight decodes as well as retained frame arrays.
/// Identical stickers share one decode, and idle entries are evicted first.
@MainActor
final class AnimatedMediaPool {
    static let shared = AnimatedMediaPool()
    nonisolated static let slotBytes = 8 * 1024 * 1024
    nonisolated static let slotLimit = 4
    private struct Entry {
        let task: Task<UIImage?, Never>
        var clients: Set<UUID>
        var touched: UInt64
        var finished = false
    }
    private var entries: [URL: Entry] = [:]
    private var clock: UInt64 = 0
    private var videos: Set<UUID> = []
    var reservedBytes: Int { entries.count * Self.slotBytes }

    func image(_ url: URL, client: UUID) async -> UIImage? {
        clock &+= 1
        if entries[url] == nil {
            while entries.count >= Self.slotLimit {
                guard let oldest = entries.filter({ $0.value.clients.isEmpty && $0.value.finished }).min(by: { $0.value.touched < $1.value.touched })?.key else { return nil }
                entries.removeValue(forKey: oldest)?.task.cancel()
            }
            entries[url] = Entry(task: Task.detached(priority: .utility) {
                await AnimatedDecodeQueue.shared.decode(url)
            }, clients: [], touched: clock)
        }
        entries[url]?.clients.insert(client); entries[url]?.touched = clock
        guard let task = entries[url]?.task else { return nil }
        let image = await task.value
        entries[url]?.finished = true
        if image == nil { entries.removeValue(forKey: url); return nil }
        guard !Task.isCancelled, entries[url]?.clients.contains(client) == true else {
            release(url, client: client); return nil
        }
        return image
    }

    func release(_ url: URL, client: UUID) {
        entries[url]?.clients.remove(client)
        if let entry = entries[url], entry.clients.isEmpty && !entry.finished { entry.task.cancel() }
    }
    func video(_ client: UUID) -> Bool {
        guard videos.contains(client) || videos.count < 2 else { return false }
        videos.insert(client); return true
    }
    func releaseVideo(_ client: UUID) { videos.remove(client) }
    func clearIdle() {
        for url in entries.keys where entries[url]?.clients.isEmpty == true {
            if entries[url]?.finished == true { entries.removeValue(forKey: url) }
            else { entries[url]?.task.cancel() }
        }
    }
}

/// A serial decoder bounds temporary ImageIO allocations even during fast scrolling.
private actor AnimatedDecodeQueue {
    static let shared = AnimatedDecodeQueue()
    func decode(_ url: URL) -> UIImage? {
        guard !Task.isCancelled else { return nil }
        return autoreleasepool { AnimatedMedia.decode(url, budget: AnimatedMediaPool.slotBytes) }
    }
}
