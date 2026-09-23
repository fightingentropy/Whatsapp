import XCTest
import UIKit
@testable import Whatsapp

@MainActor
final class ComposerTests: XCTestCase {
    func testAttachmentActionsPreserveExistingCapabilitiesAndGroupMentions() {
        XCTAssertEqual(ComposerAttachment.actions(group: false), [.photos, .camera, .document, .paste, .emoji, .gifs, .stickers])
        XCTAssertEqual(ComposerAttachment.actions(group: true).last, .mention)
        for action in ComposerAttachment.actions(group: true) where action != .stickers {
            XCTAssertNotNil(UIImage(systemName: action.symbol), action.title)
        }
    }

    func testCapturedPhotoIsUprightJPEGAndCanBeDiscarded() async throws {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let original = UIGraphicsImageRenderer(size: CGSize(width: 16, height: 8), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 16, height: 8))
        }
        let rotated = UIImage(cgImage: try XCTUnwrap(original.cgImage), scale: 1, orientation: .right)
        let url = try await Task.detached { try AttachmentImport.cameraPhoto(rotated) }.value
        defer { AttachmentImport.discard(url) }
        let data = try Data(contentsOf: url)
        let decoded = try XCTUnwrap(UIImage(data: data))
        XCTAssertEqual(Array(data.prefix(2)), [0xff, 0xd8])
        XCTAssertEqual(decoded.imageOrientation, .up)
        XCTAssertEqual(decoded.size, rotated.size)
        XCTAssertTrue(url.path.contains("cache/outgoing/"))
        AttachmentImport.discard(url)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
    }

    func testCameraCancellationDoesNotProduceAnAttachment() {
        var called = false
        let coordinator = CameraCapture.Coordinator { image in called = true; XCTAssertNil(image) }
        coordinator.imagePickerControllerDidCancel(UIImagePickerController())
        XCTAssertTrue(called)
    }
}
