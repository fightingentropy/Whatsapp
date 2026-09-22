import SwiftUI

@MainActor
enum MessageText {
    private struct MentionKey: Hashable { let token: String; let name: String }
    private final class CacheKey: NSObject {
        let source: String
        let mentions: [MentionKey]
        let size: Double
        let style: Int
        let contrast: Int
        init(_ source: String, mentions: [String: String], size: Double) {
            self.source = source
            self.mentions = mentions.map { MentionKey(token: $0.key, name: $0.value) }.sorted { $0.token < $1.token }
            self.size = size
            self.style = UITraitCollection.current.userInterfaceStyle.rawValue
            self.contrast = UITraitCollection.current.accessibilityContrast.rawValue
        }
        override var hash: Int {
            var hash = Hasher(); hash.combine(source); hash.combine(mentions)
            hash.combine(size); hash.combine(style); hash.combine(contrast); return hash.finalize()
        }
        override func isEqual(_ object: Any?) -> Bool {
            guard let other = object as? CacheKey else { return false }
            return size == other.size && style == other.style && contrast == other.contrast
                && source == other.source && mentions == other.mentions
        }
    }
    private final class Rendered {
        let text: AttributedString
        init(_ text: AttributedString) { self.text = text }
    }
    private static let cache: NSCache<CacheKey, Rendered> = {
        let cache = NSCache<CacheKey, Rendered>()
        cache.countLimit = 512; cache.totalCostLimit = 8 * 1_024 * 1_024
        return cache
    }()
    private static let expressions = [
        "```([\\s\\S]*?)```", "`([^`\\n]+)`",
        "(?<![\\p{L}\\p{N}])\\*(\\S(?:.*?\\S)?)\\*(?![\\p{L}\\p{N}])",
        "(?<![\\p{L}\\p{N}])_(\\S(?:.*?\\S)?)_(?![\\p{L}\\p{N}])",
        "(?<![\\p{L}\\p{N}])~(\\S(?:.*?\\S)?)~(?![\\p{L}\\p{N}])"
    ].map { try! NSRegularExpression(pattern: $0, options: [.dotMatchesLineSeparators]) }
    private static let lists = try! NSRegularExpression(pattern: "(?m)^(?:- |\\* |> )")
    private static let links = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

    static func clearCache() { cache.removeAllObjects() }

    static func render(_ source: String, mentions: [String: String] = [:], size: Double = 17) -> AttributedString {
        let key = CacheKey(source, mentions: mentions, size: size)
        if let cached = cache.object(forKey: key) { return cached.text }
        let result = format(source, mentions: mentions, size: size)
        // Estimate owned string/attribute storage; NSCache also responds to memory
        // pressure. Never persist formatted bodies or key by message ID alone.
        let cost = source.utf16.count * 4 + mentions.reduce(0) { $0 + ($1.key.utf16.count + $1.value.utf16.count) * 2 }
            + result.runs.count * 256 + 512
        cache.setObject(Rendered(result), forKey: key, cost: cost)
        return result
    }

    private static func format(_ source: String, mentions: [String: String], size: Double) -> AttributedString {
        let text = NSMutableAttributedString(string: source, attributes: [.font: UIFont.systemFont(ofSize: size)])
        let codeKey = NSAttributedString.Key("WhatsappCode")
        func pattern(_ expression: NSRegularExpression, font: UIFont? = nil, attributes: [NSAttributedString.Key: Any] = [:], marker: Int) {
            for match in expression.matches(in: text.string, range: NSRange(location: 0, length: text.length)).reversed() {
                guard match.numberOfRanges > 1 else { continue }
                var inCode = false
                text.enumerateAttribute(codeKey, in: match.range) { value, _, _ in if value != nil { inCode = true } }
                guard !inCode else { continue }
                let body = match.range(at: 1)
                let style = attributes
                if let font {
                    text.enumerateAttribute(.font, in: body) { existing, range, _ in
                        let traits = (existing as? UIFont)?.fontDescriptor.symbolicTraits ?? []
                        let descriptor = font.fontDescriptor.withSymbolicTraits(traits.union(font.fontDescriptor.symbolicTraits)) ?? font.fontDescriptor
                        text.addAttribute(.font, value: UIFont(descriptor: descriptor, size: font.pointSize), range: range)
                    }
                }
                text.addAttributes(style, range: body)
                text.deleteCharacters(in: NSRange(location: body.location + body.length, length: marker))
                text.deleteCharacters(in: NSRange(location: match.range.location, length: marker))
            }
        }
        pattern(expressions[0], font: .monospacedSystemFont(ofSize: size * 0.95, weight: .regular), attributes: [codeKey: true], marker: 3)
        pattern(expressions[1], font: .monospacedSystemFont(ofSize: size * 0.95, weight: .regular), attributes: [codeKey: true], marker: 1)
        pattern(expressions[2], font: .boldSystemFont(ofSize: size), marker: 1)
        pattern(expressions[3], font: .italicSystemFont(ofSize: size), marker: 1)
        pattern(expressions[4], attributes: [.strikethroughStyle: NSUnderlineStyle.single.rawValue], marker: 1)
        for match in lists.matches(in: text.string, range: NSRange(location: 0, length: text.length)).reversed() {
            guard text.attribute(codeKey, at: match.range.location, effectiveRange: nil) == nil else { continue }
            let quote = (text.string as NSString).substring(with: match.range).hasPrefix(">")
            text.replaceCharacters(in: match.range, with: quote ? "▎ " : "• ")
        }
        for (token, name) in mentions.sorted(by: { $0.key.count > $1.key.count }) {
            guard let expression = try? NSRegularExpression(pattern: "@" + NSRegularExpression.escapedPattern(for: token) + "(?![\\p{L}\\p{N}])") else { continue }
            for match in expression.matches(in: text.string, range: NSRange(location: 0, length: text.length)).reversed() {
                guard text.attribute(codeKey, at: match.range.location, effectiveRange: nil) == nil else { continue }
                text.replaceCharacters(in: match.range, with: NSAttributedString(string: "@" + name, attributes: [.font: UIFont.boldSystemFont(ofSize: size), .foregroundColor: UIColor(named: "AccentColor") ?? UIColor.systemGreen]))
            }
        }
        if let detector = links {
            for match in detector.matches(in: text.string, range: NSRange(location: 0, length: text.length)) {
                if let url = match.url, ["http", "https", "mailto", "tel"].contains(url.scheme?.lowercased() ?? ""), text.attribute(codeKey, at: match.range.location, effectiveRange: nil) == nil {
                    text.addAttributes([.link: url, .foregroundColor: UIColor.link], range: match.range)
                }
            }
        }
        return (try? AttributedString(text, including: \.uiKit)) ?? AttributedString(source)
    }

    static func plain(_ source: String, mentions: [String: String] = [:]) -> String { String(render(source, mentions: mentions).characters) }
    static func transcript(_ messages: [Message], name: (Message) -> String, mentions: (Message) -> [String: String] = { _ in [:] }) -> String {
        let formatter = DateFormatter(); formatter.dateFormat = "HH:mm, dd/MM/yyyy"
        return messages.map { "[\(formatter.string(from: Date(timeIntervalSince1970: $0.timestamp)))] \(name($0)): \(plain($0.text, mentions: mentions($0)))" }.joined(separator: "\n")
    }
}
