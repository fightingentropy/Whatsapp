import Foundation

enum EmojiCatalog {
    struct Entry: Decodable, Identifiable {
        let emoji: String
        let name: String
        let shortcodes: [String]
        var id: String { emoji }
    }
    // Generated from the same Unicode catalog used by the desktop picker.
    static let entries: [Entry] = {
        guard let url = Bundle.main.url(forResource: "emoji_catalog", withExtension: "json"),
              let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Entry].self, from: data)) ?? []
    }()
    static let byEmoji = Dictionary(entries.map { ($0.emoji, $0) }, uniquingKeysWith: { first, _ in first })
    static let byShortcode = Dictionary(entries.flatMap { entry in entry.shortcodes.map { ($0, entry.emoji) } }, uniquingKeysWith: { first, _ in first })

    static func search(_ query: String, recent: [String] = []) -> [Entry] {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if !query.isEmpty {
            return entries.filter { $0.emoji == query || $0.name.localizedStandardContains(query) || $0.shortcodes.contains { $0.contains(query) } }
        }
        let seen = Set(recent)
        return recent.compactMap { byEmoji[$0] } + entries.filter { !seen.contains($0.emoji) }
    }

    static func trailingToken(in text: String, marker: Character) -> Range<String.Index>? {
        guard let start = text.lastIndex(of: marker) else { return nil }
        guard start == text.startIndex || text[text.index(before: start)].isWhitespace else { return nil }
        let token = text[text.index(after: start)...]
        guard token.count <= 40, !token.contains(where: { $0.isWhitespace || $0 == marker }) else { return nil }
        return start..<text.endIndex
    }

    static func expandCompletedShortcode(_ text: String) -> String {
        guard text.last == ":" else { return text }
        let prefix = String(text.dropLast())
        guard let range = trailingToken(in: prefix, marker: ":"),
              let emoji = byShortcode[String(prefix[range].dropFirst())] else { return text }
        return String(prefix[..<range.lowerBound]) + emoji
    }

    static func isEmoji(_ text: String) -> Bool {
        // Include composed skin-tone and family sequences without accepting digits
        // solely because Unicode marks them as potential keycap bases.
        text.count == 1 && (byEmoji[text] != nil || text.unicodeScalars.contains { $0.properties.isEmojiPresentation } || text.contains("\u{FE0F}"))
    }
}
