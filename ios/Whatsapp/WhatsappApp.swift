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
                    TabView {
                        ChatsView().tabItem { Label("Chats", systemImage: "bubble.left.and.bubble.right.fill") }
                        SettingsView().tabItem { Label("Settings", systemImage: "gearshape.fill") }
                    }
                } else { PairingView() }
            }
            .environmentObject(store)
            .tint(.accentColor)
            .preferredColorScheme(.dark)
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

struct SettingsView: View {
    @EnvironmentObject private var store: ChatStore

    var body: some View {
        NavigationStack {
            List {
                Section {
                    HStack(spacing: 14) {
                        Image("Brand").resizable().frame(width: 54, height: 54).clipShape(RoundedRectangle(cornerRadius: 13))
                        VStack(alignment: .leading, spacing: 4) {
                            Text(store.accountName).font(.headline)
                            Text(store.connectionLabel).font(.subheadline).foregroundStyle(.secondary)
                        }
                    }.padding(.vertical, 6)
                }
                Section {
                    Toggle("Send read receipts", isOn: $store.sendReadReceipts)
                } footer: {
                    Text("Your WhatsApp account's privacy setting also applies. Reading a chat updates its unread badge across your devices.")
                }
                Section("Connection") {
                    Text("Keep Whatsapp open to receive new messages. It reconnects when you return.")
                    Text("Background notifications are not available in this version.").foregroundStyle(.secondary)
                    if !store.isDemo { Button("Reconnect") { store.reconnect() } }
                }
                Section("About") {
                    LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.13.1")
                    Text("An independent companion for your own iPhone. Messages and this device's keys stay in the app's private storage.")
                    Text("To unlink, remove “Whatsapp for iPhone” in the official app's Linked Devices settings.").foregroundStyle(.secondary)
                    Text("This first version supports chats, text replies and attachment downloads. Calls, recording and sending attachments are not included.").foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Settings")
        }
    }
}
