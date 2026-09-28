import Foundation

/// Restores file references and warms only the first visible images before a
/// local history batch reaches SwiftUI. No network, directory scans or writes.
enum CachedMedia {
    static let avatarWarmLimit = 16
    static let attachmentWarmLimit = 4
    static let restoreLimit = 64

    struct Prepared { var events: [CoreEvent]; var deferred: [String] }

    static func avatarURL(root: URL, id: String, full: Bool = false) -> URL? {
        // Keep this filename mapping identical to AppDirs::avatar_file in Rust.
        let stem = String(id.unicodeScalars.map { scalar -> Character in
            let n = scalar.value
            return (48...57).contains(n) || (65...90).contains(n) || (97...122).contains(n)
                ? Character(String(scalar)) : "_"
        })
        let url = root.appendingPathComponent("cache/avatars/\(stem)\(full ? "-full" : "").jpg")
        return readableFile(url, root: root)
    }

    static func readableFile(_ url: URL, root: URL) -> URL? {
        let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
        let prefix = root.standardizedFileURL.resolvingSymlinksInPath().path + "/"
        guard resolved.path.hasPrefix(prefix),
              let values = try? resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true, (values.fileSize ?? 0) > 0 else { return nil }
        return resolved
    }

    static func prepare(_ events: [CoreEvent], root: URL) -> [CoreEvent] {
        plan(events, root: root).events
    }

    static func restore(_ ids: [String], root: URL) -> [CoreEvent] {
        ids.compactMap { id in
            guard let url = avatarURL(root: root, id: id) else { return nil }
            var event = CoreEvent(type: "avatar")
            event.id = id; event.path = url.path; event.full = false
            event.revision = MediaFiles.inspect(url)
            return event
        }
    }

    static func plan(_ input: [CoreEvent], root: URL) -> Prepared {
        var events = input
        // Refresh descriptors on the engine queue, including same-path downloads.
        for index in events.indices {
            if let path = events[index].path, let url = MediaFiles.scoped(path, root: root) {
                events[index].revision = MediaFiles.inspect(url)
            }
            for message in events[index].messages ?? [] {
                if let path = message.mediaPath, let url = MediaFiles.scoped(path, root: root) { MediaFiles.inspect(url) }
            }
        }
        var restored: [CoreEvent] = []
        var deferred: [String] = []
        var inspected = 0
        // An explicit server result, including removal, takes precedence over
        // inferred disk entries in the same batch.
        var known = Set(events.filter { $0.type == "avatar" && $0.full != true }.compactMap(\.id))
        var warm: [(URL, Int)] = []
        var portraitCount = 0
        var attachmentCount = 0

        func restore(_ id: String, maximumSize: Int, preload: Bool) {
            guard known.insert(id).inserted else { return }
            guard inspected < restoreLimit else { deferred.append(id); return }
            inspected += 1
            guard let url = avatarURL(root: root, id: id) else { return }
            var avatar = CoreEvent(type: "avatar")
            avatar.id = id; avatar.path = url.path; avatar.full = false
            avatar.revision = MediaFiles.inspect(url)
            restored.append(avatar)
            if preload, portraitCount < avatarWarmLimit {
                portraitCount += 1
                warm.append((url, maximumSize))
            }
        }

        for event in events {
            if event.type == "chats" {
                let chats = (event.chats ?? []).sorted {
                    if $0.archived != $1.archived { return !$0.archived }
                    return $0.pinned != $1.pinned ? $0.pinned : ($0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp > $1.timestamp)
                }
                for chat in chats { restore(chat.id, maximumSize: 156, preload: !chat.archived) }
            } else if event.type == "me", let id = event.id {
                restore(id, maximumSize: 168, preload: true)
            } else if ["messages", "around", "newer"].contains(event.type) {
                // History pages end at the newest visible messages. Do not walk
                // or decode an entire conversation just to show its first screen.
                for message in (event.messages ?? []).suffix(12).reversed() {
                    restore(message.sender, maximumSize: 120, preload: true)
                    if message.kind == "image", attachmentCount < attachmentWarmLimit,
                       let path = message.mediaPath, let url = readableFile(URL(fileURLWithPath: path), root: root) {
                        attachmentCount += 1
                        warm.append((url, 720))
                    }
                }
            } else if event.type == "avatar", let path = event.path,
                      let url = readableFile(URL(fileURLWithPath: path), root: root), portraitCount < avatarWarmLimit {
                portraitCount += 1
                warm.append((url, event.full == true ? 264 : 156))
            }
        }
        for (url, size) in warm { Thumbnails.prepareStill(url, maximumSize: size) }
        return Prepared(events: restored + events, deferred: deferred)
    }
}
