import SwiftUI

enum ChatAppearance {
    static let canvas = adaptive(light: 0xF3F3EF, dark: 0x101619)
    static let incoming = adaptive(light: 0xFFFFFF, dark: 0x222B30)
    static let outgoing = adaptive(light: 0xE2F2DA, dark: 0x244139)
    static let field = adaptive(light: 0xFFFFFF, dark: 0x262D31)
    static let composerBar = adaptive(light: 0xF7F7F7, dark: 0x111111)
    static let composerField = adaptive(light: 0xFFFFFF, dark: 0x262626)
    static let attachmentPanel = adaptive(light: 0xF1F1F1, dark: 0x202020)
    static let attachmentCircle = adaptive(light: 0xE2E2E2, dark: 0x353535)
    static let composerAction = Color(red: 0.12, green: 0.79, blue: 0.38)
    static let readReceipt = adaptive(light: 0x197B9C, dark: 0x7AC4DC)
    private static let senderColors = [
        adaptive(light: 0x33786C, dark: 0x8AC6B6), adaptive(light: 0x536B99, dark: 0x9CB5DD),
        adaptive(light: 0x926A40, dark: 0xD7B58B), adaptive(light: 0x88617B, dark: 0xCBA4BB)
    ]

    static func senderColor(_ name: String) -> Color {
        // A stable, small palette keeps names and their placeholder portraits related.
        let index = name.utf8.reduce(0) { ($0 &* 31 &+ Int($1)) & 0xFFFF } % 4
        return senderColors[index]
    }

    private static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            let rgb = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(red: CGFloat((rgb >> 16) & 255) / 255,
                           green: CGFloat((rgb >> 8) & 255) / 255,
                           blue: CGFloat(rgb & 255) / 255, alpha: 1)
        })
    }
}

enum MessageGrouping {
    static func joins(_ earlier: Message, _ later: Message, calendar: Calendar = .current) -> Bool {
        let gap = later.timestamp - earlier.timestamp
        return earlier.chat == later.chat && earlier.sender == later.sender && earlier.fromMe == later.fromMe
            && earlier.kind != "revoked" && later.kind != "revoked" && gap >= 0 && gap <= 300
            && calendar.isDate(Date(timeIntervalSince1970: earlier.timestamp), inSameDayAs: Date(timeIntervalSince1970: later.timestamp))
    }
}

enum ChatDate {
    static func day(_ timestamp: TimeInterval, now: Date = .now, calendar: Calendar = .current) -> String {
        let date = Date(timeIntervalSince1970: timestamp)
        if calendar.isDate(date, inSameDayAs: now) { return String(localized: "Today") }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) {
            return String(localized: "Yesterday")
        }
        return date.formatted(calendar.component(.year, from: date) == calendar.component(.year, from: now)
            ? .dateTime.day().month(.wide) : .dateTime.day().month(.wide).year())
    }

    static func preview(_ timestamp: TimeInterval, now: Date = .now, calendar: Calendar = .current) -> String {
        guard timestamp > 0 && timestamp.isFinite else { return "" }
        let date = Date(timeIntervalSince1970: timestamp)
        if calendar.isDate(date, inSameDayAs: now) { return date.formatted(date: .omitted, time: .shortened) }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) {
            return String(localized: "Yesterday")
        }
        if let weekAgo = calendar.date(byAdding: .day, value: -7, to: now), date > weekAgo && date < now {
            return date.formatted(.dateTime.weekday(.abbreviated))
        }
        return date.formatted(date: .numeric, time: .omitted)
    }
}

struct DeliveryMark: View {
    let status: String
    var body: some View {
        Group {
            if status == "delivered" || status == "read" || status == "played" {
                ZStack {
                    Image(systemName: "checkmark").offset(x: -3)
                    Image(systemName: "checkmark").offset(x: 3)
                }.frame(width: 17)
            } else {
                Image(systemName: status == "pending" ? "clock" : status == "failed" ? "exclamationmark.circle" : "checkmark")
            }
        }
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(status == "read" || status == "played" ? ChatAppearance.readReceipt : status == "failed" ? .red : .secondary)
        .accessibilityElement(children: .ignore).accessibilityLabel(status.capitalized)
    }
}
