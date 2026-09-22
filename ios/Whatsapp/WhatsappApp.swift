import SwiftUI

@main
struct WhatsappApp: App {
    @Environment(\.scenePhase) private var phase
    @StateObject private var store: ChatStore

    init() {
        #if DEBUG
        let demo = ProcessInfo.processInfo.arguments.contains("--demo")
            || ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
        #else
        let demo = false
        #endif
        let store = ChatStore(demo: demo)
        #if DEBUG
        if demo && ProcessInfo.processInfo.arguments.contains("--demo-pairing-interrupted") {
            store.loadInterruptedPairingDemo()
        }
        #endif
        _store = StateObject(wrappedValue: store)
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if store.hasSession {
                    TabView(selection: $store.selectedTab) {
                        ChatsView().tabItem { Label("Chats", systemImage: "bubble.left.and.bubble.right.fill") }.tag("chats")
                        SettingsView().tabItem { Label("Settings", systemImage: "gearshape.fill") }.tag("settings")
                    }
                } else { PairingView() }
            }
            .environmentObject(store)
            .tint(.accentColor)
            .preferredColorScheme(store.preferences.theme == "system" ? nil : (store.preferences.theme == "light" ? .light : .dark))
            .alert("Whatsapp", isPresented: Binding(get: { store.error != nil }, set: { if !$0 { store.error = nil } })) {
                Button("OK", role: .cancel) { store.error = nil }
            } message: { Text(store.error ?? "") }
            .task { if phase == .active { store.activate() } }
            .onChange(of: phase) { _, phase in
                if phase == .active { store.activate() }
                else if phase == .inactive { store.prepareForBackground() }
                else if phase == .background { store.background() }
            }
        }
    }
}
