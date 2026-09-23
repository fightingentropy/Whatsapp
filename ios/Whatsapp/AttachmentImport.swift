import CoreTransferable
import Foundation
import UniformTypeIdentifiers
import UIKit

enum AttachmentImport {
    static let maximumBytes = 100 * 1024 * 1024
    static func directory() throws -> URL {
        let url = try CoreEngine.storageDirectory().appendingPathComponent("cache/outgoing", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    static func copy(_ source: URL) throws -> URL {
        let access = source.startAccessingSecurityScopedResource()
        defer { if access { source.stopAccessingSecurityScopedResource() } }
        let attributes = try source.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard source.isFileURL, attributes.isRegularFile == true, let size = attributes.fileSize, size <= maximumBytes else {
            throw CocoaError(.fileReadTooLarge)
        }
        let folder = try directory().appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let destination = folder.appendingPathComponent(source.lastPathComponent)
        try FileManager.default.copyItem(at: source, to: destination)
        return destination
    }

    static func pasteImage(_ data: Data) throws -> URL {
        guard data.count <= maximumBytes else { throw CocoaError(.fileReadTooLarge) }
        let destination = try directory().appendingPathComponent(UUID().uuidString + ".jpg")
        try data.write(to: destination, options: .atomic)
        return destination
    }

    // Run on the import task, not the main actor. Bake camera orientation into
    // pixels and bound the temporary bitmap before encoding the outgoing JPEG.
    static func cameraPhoto(_ image: UIImage) throws -> URL {
        guard image.size.width > 0, image.size.height > 0 else { throw CocoaError(.fileReadCorruptFile) }
        let scale = min(1, 4_096 / max(image.size.width, image.size.height))
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1; format.opaque = true
        let data = UIGraphicsImageRenderer(size: size, format: format).jpegData(withCompressionQuality: 0.9) { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return try pasteImage(data)
    }

    static func discard(_ url: URL) {
        guard let root = try? directory().resolvingSymlinksInPath(),
              url.resolvingSymlinksInPath().path.hasPrefix(root.path + "/") else { return }
        try? FileManager.default.removeItem(at: url)
    }

    static func clearTransientMedia(root: URL) {
        for name in ["outgoing", "audio-preview"] {
            try? FileManager.default.removeItem(at: root.appendingPathComponent("cache/" + name, isDirectory: true))
        }
    }
}

struct ImportedPhoto: Transferable, Sendable {
    let url: URL
    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { file in ImportedPhoto(url: try AttachmentImport.copy(file.file)) }
        FileRepresentation(importedContentType: .movie) { file in ImportedPhoto(url: try AttachmentImport.copy(file.file)) }
    }
}
