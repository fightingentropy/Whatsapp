import SwiftUI

/// Rota's building blocks: cards on paper, letter-spaced eyebrows, a
/// lowercase wordmark with a green full stop, and ink pill buttons.
struct Card<Content: View>: View {
    var padding: CGFloat
    let content: Content

    init(padding: CGFloat = 18, @ViewBuilder content: () -> Content) {
        self.padding = padding
        self.content = content()
    }

    var body: some View {
        content
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(padding)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}

/// Small uppercase, letter-spaced label.
struct Eyebrow: View {
    let text: String
    var color: Color = Theme.muted

    init(_ text: String, color: Color = Theme.muted) {
        self.text = text
        self.color = color
    }

    var body: some View {
        Text(text.uppercased())
            .font(Theme.sans(11, .medium))
            .tracking(0.8)
            .foregroundStyle(color)
            .lineLimit(1)
    }
}

struct Dot: View {
    var color: Color
    var size: CGFloat = 7

    var body: some View {
        Circle().fill(color).frame(width: size, height: size)
    }
}

struct PillStyle: ButtonStyle {
    enum Kind { case primary, secondary }
    var kind: Kind = .primary
    var large = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        let radius: CGFloat = large ? 14 : 11
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        configuration.label
            .font(Theme.sans(large ? 16 : 14, .semibold))
            .labelStyle(PillLabelStyle())
            .padding(.horizontal, large ? 20 : 12)
            .frame(height: large ? 52 : 34)
            .frame(maxWidth: large ? .infinity : nil)
            .foregroundStyle(kind == .primary ? Theme.bg : Theme.fg)
            .background(shape.fill(kind == .primary ? Theme.fg : Color.clear))
            .overlay(shape.stroke(kind == .primary ? Color.clear : Theme.line, lineWidth: 1))
            .contentShape(shape)
            .opacity(enabled ? (configuration.isPressed ? 0.85 : 1) : 0.45)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(.spring(duration: 0.25), value: configuration.isPressed)
    }
}

private struct PillLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 6) {
            configuration.icon.font(.system(size: 13, weight: .semibold))
            configuration.title
        }
    }
}

/// Lists and forms on paper with card rows, ink tint and green switches.
struct ThemedForm: ViewModifier {
    func body(content: Content) -> some View {
        content
            .scrollContentBackground(.hidden)
            .background(Theme.bg.ignoresSafeArea())
            .tint(Theme.fg)
            .toggleStyle(SwitchToggleStyle(tint: Theme.green))
            .toolbarBackground(Theme.bg, for: .navigationBar)
    }
}

extension View {
    func themedForm() -> some View { modifier(ThemedForm()) }
    /// A form section in Rota's style: card rows under an eyebrow.
    func cardRows() -> some View { listRowBackground(Theme.card).listRowSeparatorTint(Theme.line) }
}

struct Wordmark: View {
    let text: String

    var body: some View {
        Text("\(Text(text).foregroundStyle(Theme.fg))\(Text(".").foregroundStyle(Theme.green))")
            .font(Theme.sans(26, .bold))
            .tracking(-0.8)
            .accessibilityAddTraits(.isHeader)
    }
}
