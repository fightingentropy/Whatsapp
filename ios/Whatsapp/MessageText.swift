import SwiftUI

enum MessageText {
    static func render(_ source: String, mentions: [String: String] = [:], size: Double = 17) -> AttributedString {
        let text = NSMutableAttributedString(string: source, attributes: [.font: UIFont.systemFont(ofSize: size)])
        let codeKey = NSAttributedString.Key("WhatsappCode")
        func pattern(_ regex: String, font: UIFont? = nil, attributes: [NSAttributedString.Key: Any] = [:], marker: Int) {
            guard let expression = try? NSRegularExpression(pattern: regex, options: [.dotMatchesLineSeparators]) else { return }
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
        pattern("```([\\s\\S]*?)```", font: .monospacedSystemFont(ofSize: size * 0.95, weight: .regular), attributes: [codeKey: true], marker: 3)
        pattern("`([^`\\n]+)`", font: .monospacedSystemFont(ofSize: size * 0.95, weight: .regular), attributes: [codeKey: true], marker: 1)
        pattern("(?<![\\p{L}\\p{N}])\\*(\\S(?:.*?\\S)?)\\*(?![\\p{L}\\p{N}])", font: .boldSystemFont(ofSize: size), marker: 1)
        pattern("(?<![\\p{L}\\p{N}])_(\\S(?:.*?\\S)?)_(?![\\p{L}\\p{N}])", font: .italicSystemFont(ofSize: size), marker: 1)
        pattern("(?<![\\p{L}\\p{N}])~(\\S(?:.*?\\S)?)~(?![\\p{L}\\p{N}])", attributes: [.strikethroughStyle: NSUnderlineStyle.single.rawValue], marker: 1)
        if let lists = try? NSRegularExpression(pattern: "(?m)^(?:- |\\* |> )") {
            for match in lists.matches(in: text.string, range: NSRange(location: 0, length: text.length)).reversed() {
                guard text.attribute(codeKey, at: match.range.location, effectiveRange: nil) == nil else { continue }
                let quote = (text.string as NSString).substring(with: match.range).hasPrefix(">")
                text.replaceCharacters(in: match.range, with: quote ? "▎ " : "• ")
            }
        }
        for (token, name) in mentions.sorted(by: { $0.key.count > $1.key.count }) {
            guard let expression = try? NSRegularExpression(pattern: "@" + NSRegularExpression.escapedPattern(for: token) + "(?![\\p{L}\\p{N}])") else { continue }
            for match in expression.matches(in: text.string, range: NSRange(location: 0, length: text.length)).reversed() {
                guard text.attribute(codeKey, at: match.range.location, effectiveRange: nil) == nil else { continue }
                text.replaceCharacters(in: match.range, with: NSAttributedString(string: "@" + name, attributes: [.font: UIFont.boldSystemFont(ofSize: size), .foregroundColor: UIColor.systemGreen]))
            }
        }
        if let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) {
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
