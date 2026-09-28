import XCTest
@testable import Whatsapp

@MainActor
final class DraftStorageTests: XCTestCase {
    func testEditingAnotherMessageDoesNotReplaceTheUnsentDraft() {
        let store = ChatStore(demo: true)
        store.open("weekend@g.us")
        store.draftBeforeEditing = "Unsent draft"
        store.editing = store.messages.last
        store.saveDraft("Changed message being edited", chat: "weekend@g.us")
        XCTAssertEqual(store.drafts["weekend@g.us"], "Unsent draft")
    }

    func testDraftAttachmentsAndInterruptedUploadsSurviveNewStore() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("Drafts-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("cache/outgoing/fixture/file.pdf")
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("synthetic PDF".utf8).write(to: file)
        let disk = DraftStorage(root: root)
        let first = ChatStore(demo: true, draftStorage: disk)
        first.drafts["fixture@lid"] = "Keep this caption"
        first.attachments["fixture@lid"] = [PendingAttachment(url: file)]
        first.outgoingAttachments = [OutgoingAttachment(id: "pending", chat: "other@lid", file: PendingAttachment(url: file), caption: "Upload caption", mentions: [], state: .uploading)]
        let saved = await withCheckedContinuation { continuation in first.flushDrafts { continuation.resume(returning: $0) } }
        XCTAssertTrue(saved)
        let second = ChatStore(demo: true, draftStorage: disk)
        defer { first.draftSaveWork?.cancel(); second.draftSaveWork?.cancel() }
        XCTAssertEqual(second.drafts["fixture@lid"], "Keep this caption")
        XCTAssertEqual(second.attachments["fixture@lid"]?.first?.url, file)
        XCTAssertEqual(second.outgoingAttachments.first?.state, .failed)
        XCTAssertEqual(second.outgoingAttachments.first?.caption, "Upload caption")
        XCTAssertNil(second.uploadingAttachment, "Never automatically resend an interrupted upload")
    }

    func testPruningKeepsReferencedDraftsRecentImportsAndArchivedMedia() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("Prune-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        func file(_ relative: String, old: Bool) throws -> URL {
            let url = root.appendingPathComponent(relative)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data([1]).write(to: url)
            if old { try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSinceNow: -9 * 86_400)], ofItemAtPath: url.path) }
            return url
        }
        let kept = try file("cache/outgoing/retained.pdf", old: true)
        let orphan = try file("cache/outgoing/orphan.pdf", old: true)
        let recent = try file("cache/outgoing/recent.pdf", old: false)
        let original = try file("cache/media/original.pdf", old: true)
        AttachmentImport.prune(root: root, retaining: [kept])
        XCTAssertFalse(FileManager.default.fileExists(atPath: orphan.path))
        for url in [kept, recent, original] { XCTAssertTrue(FileManager.default.fileExists(atPath: url.path)) }
        XCTAssertNil(AttachmentImport.restoredURL(original, root: root))
        let escape = URL(fileURLWithPath: root.path + "/cache/outgoing/../../private")
        XCTAssertNil(AttachmentImport.restoredURL(escape, root: root))
    }
}
