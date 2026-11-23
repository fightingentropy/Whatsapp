import SwiftUI

/// The two top-level pages. Raw values match `ChatStore.selectedTab`.
enum AppTab: String, CaseIterable, Identifiable {
    case chats, settings
    var id: String { rawValue }

    var title: LocalizedStringKey {
        switch self {
        case .chats: "Chats"
        case .settings: "Settings"
        }
    }

    func icon(selected: Bool) -> String {
        switch self {
        case .chats: selected ? "bubble.left.and.bubble.right.fill" : "bubble.left.and.bubble.right"
        case .settings: selected ? "gearshape.fill" : "gearshape"
        }
    }
}

/// Bottom chrome shared with the Rota app: a full-width dark frosted band that
/// fades into the content, with filled symbols for the selected page.
struct GlassTabBar: View {
    @Binding var selection: String
    var unread = 0

    var body: some View {
        HStack(spacing: 0) {
            ForEach(AppTab.allCases) { tab in
                TabButton(tab: tab, selected: selection == tab.rawValue,
                          badge: tab == .chats ? unread : 0) {
                    selection = tab.rawValue
                }
            }
        }
        .padding(.horizontal, 8)
        .padding(.top, 26)
        .padding(.bottom, 2)
        .background(alignment: .bottom) {
            GlassBackdrop().ignoresSafeArea(edges: .bottom)
        }
        .environment(\.colorScheme, .dark)
    }
}

private struct GlassBackdrop: View {
    var body: some View {
        ZStack {
            Rectangle().fill(.ultraThinMaterial)
            LinearGradient(
                stops: [
                    .init(color: .black.opacity(0.35), location: 0),
                    .init(color: .black.opacity(0.75), location: 0.45),
                    .init(color: .black.opacity(0.92), location: 1),
                ],
                startPoint: .top, endPoint: .bottom
            )
        }
        .mask(
            LinearGradient(
                stops: [
                    .init(color: .clear, location: 0),
                    .init(color: .black.opacity(0.85), location: 0.22),
                    .init(color: .black, location: 0.4),
                ],
                startPoint: .top, endPoint: .bottom
            )
        )
    }
}

private struct TabButton: View {
    let tab: AppTab
    let selected: Bool
    let badge: Int
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 5) {
                Image(systemName: tab.icon(selected: selected))
                    .font(.system(size: 22, weight: selected ? .semibold : .regular))
                    .frame(height: 26)
                    .contentTransition(.symbolEffect(.replace))
                    .overlay(alignment: .topTrailing) {
                        if badge > 0 {
                            Text(badge > 99 ? "99+" : "\(badge)")
                                .font(.system(size: 10, weight: .semibold).monospacedDigit())
                                .foregroundStyle(.black)
                                .padding(.horizontal, 5)
                                .frame(minWidth: 17, minHeight: 17)
                                .background(Capsule().fill(.white))
                                .offset(x: 12, y: -4)
                        }
                    }
                Text(tab.title)
                    .font(.system(size: 10.5, weight: selected ? .semibold : .medium))
            }
            .foregroundStyle(selected ? Color.white : Color.white.opacity(0.55))
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(TabPressStyle())
        .sensoryFeedback(.selection, trigger: selected)
        .accessibilityLabel(Text(tab.title))
        .accessibilityValue(badge > 0 ? Text("\(badge) unread") : Text(""))
        .accessibilityIdentifier("tab-\(tab.rawValue)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

private struct TabPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.9 : 1)
            .animation(.spring(duration: 0.2), value: configuration.isPressed)
    }
}
