import Foundation

/// File inspection happens when an engine batch arrives. Rebuilding SwiftUI
/// rows only consults these bounded, thread-safe caches.
enum MediaFiles {
    private final class File: NSObject {
        let url: URL
        let revision: String
        init(_ url: URL, revision: String) { self.url = url; self.revision = revision }
    }
    private static let files: NSCache<NSString, File> = {
        let cache = NSCache<NSString, File>(); cache.countLimit = 4_096; return cache
    }()
    private static let roots: NSCache<NSString, NSURL> = {
        let cache = NSCache<NSString, NSURL>(); cache.countLimit = 16; return cache
    }()

    static func clear() { files.removeAllObjects(); roots.removeAllObjects() }
    static func revision(_ url: URL) -> String? { files.object(forKey: url.path as NSString)?.revision }

    @discardableResult static func inspect(_ url: URL) -> String {
        let revision = Thumbnails.revision(url)
        files.setObject(File(url, revision: revision), forKey: url.path as NSString)
        return revision
    }

    static func scoped(_ path: String, root: URL) -> URL? {
        let key = (root.path + "|" + path) as NSString
        if let file = files.object(forKey: key) { return file.url }
        let prefix: URL
        if let known = roots.object(forKey: root.path as NSString) { prefix = known as URL }
        else {
            prefix = root.standardizedFileURL.resolvingSymlinksInPath()
            roots.setObject(prefix as NSURL, forKey: root.path as NSString)
        }
        let url = URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath()
        guard url.path.hasPrefix(prefix.path + "/") else { return nil }
        files.setObject(File(url, revision: ""), forKey: key)
        return url
    }
}
