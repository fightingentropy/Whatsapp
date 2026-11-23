import SwiftUI
import UIKit

/// The palette shared with the Rota app and health.erlin.org: warm near-black
/// ink on paper, low-contrast cards, green only for status, mono numerals.
enum Theme {
    static let bg     = Color(light: 0xF5F3EF, dark: 0x100E0C)
    static let card   = Color(light: 0xEBE8E3, dark: 0x191714)
    static let raised = Color(light: 0xE0DCD6, dark: 0x24211E)
    static let line   = Color(light: 0xD6D2CB, dark: 0x2B2825)
    static let fg     = Color(light: 0x171513, dark: 0xF0EEEB)
    static let muted  = Color(light: 0x76726C, dark: 0x9B9893)
    static let faint  = Color(light: 0xA9A59F, dark: 0x5C5853)
    static let green  = Color(light: 0x1E9B52, dark: 0x62D286)
    static let amber  = Color(light: 0xB86E0A, dark: 0xF0B54C)
    static let red    = Color(light: 0xC8432D, dark: 0xF07A63)

    static func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func sans(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }
}

extension Color {
    init(light: UInt32, dark: UInt32) {
        self = Color(uiColor: UIColor { traits in
            UIColor(hex: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

extension UIColor {
    convenience init(hex: UInt32) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: 1
        )
    }
}
