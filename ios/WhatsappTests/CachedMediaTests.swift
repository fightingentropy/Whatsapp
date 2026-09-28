import XCTest
import SwiftUI
@testable import Whatsapp

@MainActor
final class CachedMediaTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("CachedMedia-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        Thumbnails.clear()
    }

    override func tearDownWithError() throws {
        Thumbnails.clear()
        try FileManager.default.removeItem(at: root)
    }

    @discardableResult private func picture(_ relativePath: String, color: UIColor = .red) throws -> URL {
        let url = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: 64, height: 64), format: format).image { context in
            color.setFill(); context.fill(CGRect(x: 0, y: 0, width: 64, height: 64))
        }
        try XCTUnwrap(image.pngData()).write(to: url, options: .atomic)
        return url
    }

    private func chat(_ id: String, time: Double = 1, pinned: Bool = false, archived: Bool = false) -> Chat {
        Chat(id: id, name: "Fixture", kind: "direct", timestamp: time, unread: 0,
             archived: archived, pinned: pinned, readOnly: false, preview: "Saved message")
    }

    func testColdRestorationPublishesPicturesBeforeChatsWithoutAConnection() throws {
        let url = try picture("cache/avatars/alex_lid.jpg")
        var chats = CoreEvent(type: "chats"); chats.chats = [chat("alex@lid")]
        var link = CoreEvent(type: "link"); link.status = "connecting"
        // Repeat with an empty memory cache, as after terminating the process.
        for _ in 0..<2 {
            Thumbnails.clear()
            let events = CachedMedia.prepare([link, chats], root: root)
            XCTAssertEqual(events.first?.type, "avatar")
            XCTAssertEqual(events.first?.path, url.resolvingSymlinksInPath().path)
            XCTAssertEqual(events.last?.chats, chats.chats)
            XCTAssertNotNil(Thumbnails.cached(.init(url, maximumSize: 156)))
        }
    }

    func testLargeChatListRestoresFirstScreenThenDeferredPortraits() throws {
        var batch = CoreEvent(type: "chats")
        batch.chats = try (0..<100).map { index in
            try picture("cache/avatars/person\(index)_lid.jpg")
            return chat("person\(index)@lid", time: Double(100 - index), archived: index < 20)
        }
        let prepared = CachedMedia.plan([batch], root: root)
        XCTAssertEqual(prepared.events.filter { $0.type == "avatar" }.count, CachedMedia.restoreLimit)
        XCTAssertEqual(prepared.events.first?.id, "person20@lid", "Visible chats take precedence over archived portraits")
        XCTAssertEqual(prepared.events.last?.chats, batch.chats)
        XCTAssertEqual(prepared.deferred.count, 100 - CachedMedia.restoreLimit)
        let deferred = CachedMedia.restore(prepared.deferred, root: root)
        XCTAssertEqual(deferred.count, prepared.deferred.count)
        XCTAssertTrue(deferred.allSatisfy { $0.revision != nil })
        let last = root.appendingPathComponent("cache/avatars/person99_lid.jpg")
        XCTAssertNil(Thumbnails.cached(.init(last, maximumSize: 156)), "Offscreen portraits are not eagerly decoded")
    }

    func testWarmImageIsPresentInFirstRenderAndSurvivesMemoryCacheEviction() throws {
        let url = try picture("cache/avatars/alex_lid.jpg")
        Thumbnails.prepareStill(url, maximumSize: 156)
        let view = LocalImage(url: url, maximumSize: 156)
        Thumbnails.clear()
        // No asynchronous SwiftUI task runs before this synchronous render.
        let renderer = ImageRenderer(content: view.frame(width: 20, height: 20))
        let cg = try XCTUnwrap(renderer.cgImage)
        var pixel = [UInt8](repeating: 0, count: 4)
        let context = try XCTUnwrap(CGContext(data: &pixel, width: 1, height: 1, bitsPerComponent: 8,
            bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(cg, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        XCTAssertGreaterThan(pixel[0], 240)
        XCTAssertLessThan(pixel[1], 15)
        XCTAssertLessThan(pixel[2], 15)
    }

    func testSameFilenameRefreshAndDifferentSizesDoNotReuseOldDecode() throws {
        let url = try picture("cache/avatars/alex_lid.jpg")
        let original = Thumbnails.Request(url, maximumSize: 156)
        Thumbnails.prepareStill(url, maximumSize: 156)
        try picture("cache/avatars/alex_lid.jpg", color: .blue)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSinceNow: 10)], ofItemAtPath: url.path)
        let refreshed = Thumbnails.Request(url, maximumSize: 156)
        XCTAssertNotEqual(original, refreshed)
        XCTAssertNil(Thumbnails.cached(refreshed))
        XCTAssertNotNil(Thumbnails.prepareStill(url, maximumSize: 156))
        XCTAssertNotNil(Thumbnails.cached(refreshed))
        XCTAssertNil(Thumbnails.cached(.init(url, maximumSize: 264)))
    }

    func testMissingRemovedAndOutsideContainerFilesAreNotRestored() throws {
        let url = try picture("cache/avatars/alex_lid.jpg")
        var chats = CoreEvent(type: "chats"); chats.chats = [chat("alex@lid")]
        var removal = CoreEvent(type: "avatar"); removal.id = "alex@lid"; removal.full = false
        XCTAssertEqual(CachedMedia.prepare([chats, removal], root: root).filter { $0.type == "avatar" }.count, 1)
        try Data().write(to: url)
        XCTAssertNil(CachedMedia.avatarURL(root: root, id: "alex@lid"))
        XCTAssertEqual(CachedMedia.prepare([chats], root: root).count, 1)
        XCTAssertNil(CachedMedia.avatarURL(root: root, id: "missing@lid"))
        let outside = try picture("outside.png")
        let limitedRoot = root.appendingPathComponent("isolated")
        let link = limitedRoot.appendingPathComponent("cache/avatars/alex_lid.jpg")
        try FileManager.default.createDirectory(at: link.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)
        XCTAssertNil(CachedMedia.avatarURL(root: limitedRoot, id: "alex@lid"))
    }

    func testFilenameMatchesSharedWorkerIncludingUnicodeScalarsAndFullSize() throws {
        let preview = try picture("cache/avatars/a___lid.jpg")
        let full = try picture("cache/avatars/a___lid-full.jpg")
        XCTAssertEqual(CachedMedia.avatarURL(root: root, id: "aé🦉@lid"), preview.resolvingSymlinksInPath())
        XCTAssertEqual(CachedMedia.avatarURL(root: root, id: "aé🦉@lid", full: true), full.resolvingSymlinksInPath())
    }

    func testPreloadingIsBoundedAndPrioritizesVisibleChats() throws {
        let limit = CachedMedia.avatarWarmLimit
        var chats = CoreEvent(type: "chats")
        chats.chats = try (0..<(limit + 3)).map { n in
            try picture("cache/avatars/fixture\(n)_lid.jpg")
            return chat("fixture\(n)@lid", time: Double(n), pinned: n == 0, archived: n == limit + 2)
        }
        let events = CachedMedia.prepare([chats], root: root)
        XCTAssertEqual(events.filter { $0.type == "avatar" }.count, limit + 3)
        let warmed = (0..<(limit + 3)).filter { n in
            Thumbnails.cached(.init(root.appendingPathComponent("cache/avatars/fixture\(n)_lid.jpg"), maximumSize: 156)) != nil
        }
        XCTAssertEqual(warmed.count, limit)
        XCTAssertTrue(warmed.contains(0))
        XCTAssertTrue(warmed.contains(limit + 1))
        XCTAssertFalse(warmed.contains(limit + 2))
    }

    func testRestoredHistoryPreloadsRecentDownloadedPhotosOnly() throws {
        var event = CoreEvent(type: "messages"); event.chat = "fixture@lid"
        event.messages = try (0..<10).map { n in
            let url = try picture("cache/media/photo\(n).png")
            var message = ChatStore.sampleMessage(id: "photo\(n)", chat: "fixture@lid", text: "Photo", fromMe: false, time: Double(n))
            message.kind = "image"; message.mediaPath = url.path
            return message
        }
        _ = CachedMedia.prepare([event], root: root)
        for n in 0..<10 {
            let image = Thumbnails.cached(.init(root.appendingPathComponent("cache/media/photo\(n).png"), maximumSize: 720))
            XCTAssertEqual(image != nil, n >= 10 - CachedMedia.attachmentWarmLimit)
        }
    }
}
