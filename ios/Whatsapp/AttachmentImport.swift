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
