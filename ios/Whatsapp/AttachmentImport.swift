import CoreTransferable
import Foundation
import UniformTypeIdentifiers
import UIKit
import ImageIO

enum AttachmentImport {
    static let maximumBytes = 100 * 1024 * 1024
    static let maximumVoiceBytes = 48_000 * 4 * 600
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
        do { try FileManager.default.copyItem(at: source, to: destination) }
        catch { try? FileManager.default.removeItem(at: folder); throw error }
        return destination
    }

    static func copyAll(_ sources: [URL]) throws -> [URL] {
        var copied: [URL] = []
        do { for source in sources { copied.append(try copy(source)) }; return copied }
        catch { copied.forEach(discard); throw error }
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
        let parent = url.deletingLastPathComponent()
        if parent != root, (try? FileManager.default.contentsOfDirectory(atPath: parent.path).isEmpty) == true {
            try? FileManager.default.removeItem(at: parent)
        }
    }

    static func restoredURL(_ url: URL, root: URL, maximumSize: Int = maximumBytes) -> URL? {
        let outgoing = root.appendingPathComponent("cache/outgoing", isDirectory: true).resolvingSymlinksInPath()
        guard let range = url.path.range(of: "/cache/outgoing/", options: .backwards) else { return nil }
        let relative = String(url.path[range.upperBound...])
        let candidate = outgoing.appendingPathComponent(relative).standardizedFileURL.resolvingSymlinksInPath()
        guard candidate.path.hasPrefix(outgoing.path + "/"),
              let values = try? candidate.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true, let size = values.fileSize, size <= maximumSize else { return nil }
        return candidate
    }

    static func prune(root: URL, retaining urls: Set<URL>, now: Date = Date()) {
        let folder = root.appendingPathComponent("cache/outgoing", isDirectory: true).resolvingSymlinksInPath()
        let retained = Set(urls.map { $0.resolvingSymlinksInPath() })
        let keys: [URLResourceKey] = [.isRegularFileKey, .isSymbolicLinkKey, .contentModificationDateKey]
        guard let files = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles]) else { return }
        for case let file as URL in files {
            guard let values = try? file.resourceValues(forKeys: Set(keys)), values.isSymbolicLink != true,
                  values.isRegularFile == true, !retained.contains(file.resolvingSymlinksInPath()),
                  let modified = values.contentModificationDate, now.timeIntervalSince(modified) > 7 * 86_400 else { continue }
            try? FileManager.default.removeItem(at: file)
        }
    }

    static func photo(_ source: URL) throws -> URL {
        let access = source.startAccessingSecurityScopedResource()
        defer { if access { source.stopAccessingSecurityScopedResource() } }
        guard let image = CGImageSourceCreateWithURL(source as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetCount(image) == 1,
              let frame = CGImageSourceCreateThumbnailAtIndex(image, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 4096, kCGImageSourceShouldCacheImmediately: true
              ] as CFDictionary), let data = UIImage(cgImage: frame).jpegData(compressionQuality: 0.9) else { return try copy(source) }
        return try pasteImage(data)
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
        FileRepresentation(importedContentType: .image) { file in ImportedPhoto(url: try AttachmentImport.photo(file.file)) }
        FileRepresentation(importedContentType: .movie) { file in ImportedPhoto(url: try AttachmentImport.copy(file.file)) }
    }
}
