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

struct SettingsView: View {
    @EnvironmentObject private var store: ChatStore
    @State private var unlinkPresented = false

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
                Section("Appearance") {
                    Picker("Theme", selection: $store.preferences.theme) {
                        Text("Dark").tag("dark"); Text("Light").tag("light"); Text("System").tag("system")
                    }
                    VStack(alignment: .leading) {
                        LabeledContent("Message text size", value: "\(Int(store.preferences.textSize))")
                        Slider(value: $store.preferences.textSize, in: 14...24, step: 1)
                    }
                    Toggle("Sender pictures in every chat", isOn: $store.preferences.senderPictures)
                }
                Section {
                    Toggle("Send read receipts", isOn: $store.sendReadReceipts)
                    Toggle("Show when you are typing", isOn: $store.preferences.sendTyping)
                        .onChange(of: store.preferences.sendTyping) { _, enabled in if !enabled { store.stopComposing() } }
                    Toggle("Download attachments automatically", isOn: $store.preferences.autoDownload)
                    Toggle("Prefer saved contact names", isOn: $store.preferences.contactNames)
                    Toggle("Save contacts to phone address book", isOn: $store.preferences.saveContactsToPhone)
                } footer: {
                    Text("Automatic downloads cover visible attachments up to 64 MB. Your WhatsApp privacy setting also applies to read and played receipts. Reading a chat updates its unread badge across devices.")
                }
                Section {
                    Toggle("Notify about new messages", isOn: $store.preferences.notifications)
                        .onChange(of: store.preferences.notifications) { _, enabled in
                            guard enabled, !store.isDemo else { return }
                            Task { if !(await store.notifications.request()) { store.preferences.notifications = false; store.error = "Enable notifications for Whatsapp in iPhone Settings." } }
                        }
                } header: { Text("Notifications") } footer: {
                    Text("Alerts work while this app is connected, including its brief background allowance. They stop when iOS suspends the app. Muted chats do not notify you.")
                }
                Section("GIF search") {
                    SecureField("GIPHY API key", text: $store.preferences.giphyKey).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Link("Get a GIPHY API key", destination: URL(string: "https://developers.giphy.com/")!)
                }
                Section("Connection") {
                    Text("Keep Whatsapp open to receive new messages. It reconnects when you return.")
                    if !store.isDemo { Button("Reconnect") { store.reconnect() } }
                    if let id = store.accountID { NavigationLink("Your profile") { ChatDetails(id: id) } }
                    Button("Unlink this iPhone", role: .destructive) { unlinkPresented = true }
                }
                Section("About") {
                    LabeledContent("Version", value: "\(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.13.1") (\(Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "3"))")
                    Text("An independent companion for your own iPhone. Messages and this device's keys stay in the app's private storage.")
                    Text("To unlink, remove “Whatsapp for iPhone” in the official app's Linked Devices settings.").foregroundStyle(.secondary)
                    Text("Calls, status posts and group administration are not supported by this client.").foregroundStyle(.secondary)
                    Link("Project and updates", destination: URL(string: "https://github.com/fightingentropy/Whatsapp")!)
                }
            }
            .navigationTitle("Settings")
            .confirmationDialog("Unlink this iPhone and clear its local chat history?", isPresented: $unlinkPresented, titleVisibility: .visible) {
                Button("Unlink this iPhone", role: .destructive) { store.unlink() }
            } message: { Text("Your primary WhatsApp account and messages on other devices will stay intact.") }
        }
    }
}
