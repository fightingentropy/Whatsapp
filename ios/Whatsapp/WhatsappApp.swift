import SwiftUI

@main
struct WhatsappApp: App {
    @Environment(\.scenePhase) private var phase
    @State private var store: ChatStore

    init() {
        #if BENCHMARK
        // Separate container, optimized code, and no live account even if the
        // test runner forgets its launch arguments.
        let demo = true
        #elseif DEBUG
        let demo = ProcessInfo.processInfo.arguments.contains("--demo")
            || ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
        #else
        let demo = false
        #endif
        let store = ChatStore(demo: demo)
        #if DEBUG || BENCHMARK
        if demo && ProcessInfo.processInfo.arguments.contains("--performance-fixture") {
            store.loadPerformanceDemo()
        }
        if demo && ProcessInfo.processInfo.arguments.contains("--demo-pairing-interrupted") {
            store.loadInterruptedPairingDemo()
        }
        if demo && ProcessInfo.processInfo.arguments.contains("--demo-cached-pictures") {
            store.loadCachedPicturesDemo()
        }
        if demo && ProcessInfo.processInfo.arguments.contains("--demo-inline-video") { store.loadInlineVideoDemo() }
        if demo && ProcessInfo.processInfo.arguments.contains("--demo-voice-recovery") { store.loadVoiceRecoveryDemo() }
        #endif
        _store = State(initialValue: store)
    }

    var body: some Scene {
        @Bindable var store = store
        WindowGroup {
            Group {
                if store.hasSession {
                    RootTabs()
                } else { PairingView() }
            }
            .environment(store)
            .tint(Theme.fg)
            .preferredColorScheme(store.preferences.theme == "system" ? nil : (store.preferences.theme == "light" ? .light : .dark))
            .alert("Whatsapp", isPresented: Binding(get: { store.error != nil }, set: { if !$0 && store.error != nil { store.error = nil } })) {
                Button("OK", role: .cancel) { store.error = nil }
            } message: { Text(store.error ?? "") }
            .task { if phase == .active { store.activate() } }
            .onChange(of: phase) { _, phase in
                if phase == .active { store.activate() }
                else if phase == .inactive { store.prepareForBackground() }
                else if phase == .background { store.background() }
            }
            .onReceive(NotificationCenter.default.publisher(for: UIApplication.didReceiveMemoryWarningNotification)) { _ in
                store.clearMemoryCaches()
            }
        }
        .backgroundTask(.appRefresh(BackgroundRefresh.identifier)) { [store] in
            await store.refreshInBackground()
        }
    }
}

/// Both pages stay alive so navigation and scroll positions survive switching,
/// under the Rota-style glass bar. An open conversation owns the bottom edge.
private struct RootTabs: View {
    @Environment(ChatStore.self) private var store

    var body: some View {
        ZStack {
            page(.chats) { ChatsView() }
            page(.settings) { SettingsView() }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            let conversation = store.selectedTab == AppTab.chats.rawValue && !store.navigation.isEmpty
            VStack(spacing: 0) {
                if !conversation {
                    TabBarHost().transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.snappy(duration: 0.25), value: conversation)
        }
    }

    private func page<V: View>(_ tab: AppTab, @ViewBuilder _ content: () -> V) -> some View {
        let shown = store.selectedTab == tab.rawValue
        return content()
            .opacity(shown ? 1 : 0)
            .allowsHitTesting(shown)
            .accessibilityHidden(!shown)
    }
}

/// Reads chat state on its own, so unread changes redraw only the bar.
private struct TabBarHost: View {
    @Environment(ChatStore.self) private var store

    var body: some View {
        @Bindable var store = store
        let unread = store.chats.reduce(0) { $0 + ($1.unread > 0 && !$1.archived && !$1.muted ? 1 : 0) }
        GlassTabBar(selection: $store.selectedTab, unread: unread)
    }
}
