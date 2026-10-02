import UIKit

/// One decode per file revision/size, with two active jobs. A disappearing row
/// releases only its own interest; the last reader cancels queued/native work.
actor ThumbnailLoader {
    private struct Job {
        let id = UUID()
        var readers: [UUID: CheckedContinuation<UIImage?, Never>] = [:]
    }
    private var jobs: [Thumbnails.Request: Job] = [:]
    private var queue: [Thumbnails.Request] = []
    private var running: [UUID: Task<Void, Never>] = [:]
    private let limit: Int
    private let decode: @Sendable (Thumbnails.Request) async -> UIImage?
    var pendingReaderCount: Int { jobs.values.reduce(0) { $0 + $1.readers.count } }

    init(limit: Int = 2, decode: @escaping @Sendable (Thumbnails.Request) async -> UIImage? = { await Thumbnails.decode($0) }) {
        self.limit = max(1, limit); self.decode = decode
    }

    func load(_ request: Thumbnails.Request) async -> UIImage? {
        guard !Task.isCancelled else { return nil }
        if let image = Thumbnails.cached(request) { return image }
        let reader = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                guard !Task.isCancelled else { continuation.resume(returning: nil); return }
                if jobs[request] == nil {
                    guard jobs.count < 256 else { continuation.resume(returning: nil); return }
                    jobs[request] = Job(); queue.append(request)
                }
                jobs[request]?.readers[reader] = continuation
                pump()
            }
        } onCancel: { Task { await self.cancel(request, reader: reader) } }
    }

    private func cancel(_ request: Thumbnails.Request, reader: UUID) {
        guard let continuation = jobs[request]?.readers.removeValue(forKey: reader) else { return }
        continuation.resume(returning: nil)
        if let job = jobs[request], job.readers.isEmpty {
            jobs.removeValue(forKey: request)
            queue.removeAll { $0 == request }
            running[job.id]?.cancel()
        }
        pump()
    }

    private func pump() {
        while running.count < limit, !queue.isEmpty {
            let request = queue.removeFirst()
            guard let job = jobs[request] else { continue }
            let decode = decode
            running[job.id] = Task.detached(priority: .utility) {
                let image = await decode(request)
                await self.finish(request, id: job.id, image: Task.isCancelled ? nil : image)
            }
        }
    }

    private func finish(_ request: Thumbnails.Request, id: UUID, image: UIImage?) {
        running.removeValue(forKey: id)
        if jobs[request]?.id == id, let job = jobs.removeValue(forKey: request) {
            if let image { Thumbnails.keep(image, for: request) }
            job.readers.values.forEach { $0.resume(returning: image) }
        }
        pump()
    }
}
