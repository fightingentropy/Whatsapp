import SwiftUI

struct SettingsView: View {
    @Environment(ChatStore.self) private var store
    @State private var unlinkPresented = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if let id = store.accountID {
                        NavigationLink { ChatDetails(id: id) } label: { account }
                    } else { account }
                } header: {
                    VStack(alignment: .leading, spacing: 18) {
                        Wordmark(text: "settings").textCase(nil).padding(.leading, -16)
                        Eyebrow("Account")
                    }.padding(.bottom, 4)
                }.cardRows()
                Section {
                    NavigationLink { AppearanceSettings() } label: { SettingsLabel("Appearance", icon: "paintpalette") }
                    NavigationLink { PrivacySettings() } label: { SettingsLabel("Privacy & contacts", icon: "hand.raised") }
                    NavigationLink { MediaSettings() } label: { SettingsLabel("Media & downloads", icon: "photo.on.rectangle") }
                    NavigationLink { NotificationSettings() } label: { SettingsLabel("Notifications", icon: "bell") }
                } header: { Eyebrow("Preferences") }.cardRows()
                Section {
                    if !store.isDemo {
                        Button { store.reconnect() } label: { SettingsLabel("Reconnect", icon: "arrow.triangle.2.circlepath") }
                    }
                    NavigationLink { AboutSettings() } label: { SettingsLabel("About Whatsapp", icon: "info.circle") }
                } header: { Eyebrow("Connection") } footer: {
                    Text("Whatsapp catches up in the background when iOS allows it, and reconnects as soon as you open it.")
                }.cardRows()
                Section {
                    Button(role: .destructive) { unlinkPresented = true } label: {
                        Label("Unlink this iPhone", systemImage: "rectangle.portrait.and.arrow.right")
                    }.foregroundStyle(Theme.red)
                } header: { Eyebrow("Device") }.cardRows()
            }
            .themedForm()
            .toolbar(.hidden, for: .navigationBar)
            .confirmationDialog("Unlink this iPhone and clear its local chat history?", isPresented: $unlinkPresented, titleVisibility: .visible) {
                Button("Unlink this iPhone", role: .destructive) { store.unlink() }
            } message: { Text("Your primary WhatsApp account and messages on other devices will stay intact.") }
        }
    }

    private var account: some View {
        HStack(spacing: 14) {
            AvatarView(name: store.accountName, url: store.accountID.flatMap { store.avatarURL($0) }, size: 52)
            VStack(alignment: .leading, spacing: 5) {
                Text(store.accountName).font(Theme.sans(17, .semibold)).foregroundStyle(Theme.fg)
                HStack(spacing: 6) {
                    Dot(color: store.connected ? Theme.green : (store.isDemo ? Theme.muted : Theme.amber))
                    Text(store.connectionLabel).font(Theme.mono(12.5)).foregroundStyle(Theme.muted)
                }
            }
        }.padding(.vertical, 6)
    }
}

private struct SettingsLabel: View {
    let title: String
    let icon: String
    init(_ title: String, icon: String) { self.title = title; self.icon = icon }
    var body: some View {
        Label { Text(title).foregroundStyle(Theme.fg) } icon: { Image(systemName: icon).font(.system(size: 17, weight: .regular)).foregroundStyle(Theme.muted) }
            .padding(.vertical, 3)
    }
}

private struct AppearanceSettings: View {
    @Environment(ChatStore.self) private var store
    var body: some View {
        @Bindable var store = store
        List {
            Section {
                VStack(alignment: .trailing, spacing: 3) {
                    Text("A little more room to read.").font(.system(size: store.preferences.textSize))
                    HStack(spacing: 4) { Text("10:24"); DeliveryMark(status: "read") }.font(.system(size: 10.5)).foregroundStyle(.secondary)
                }.padding(.horizontal, 12).padding(.vertical, 9)
                    .background(ChatAppearance.outgoing, in: RoundedRectangle(cornerRadius: 16))
                    .frame(maxWidth: .infinity, alignment: .trailing).padding(.vertical, 14)
                    .accessibilityLabel("Message text preview")
            } header: { Eyebrow("Preview") }.listRowBackground(Theme.bg)
            Section {
                Picker("Theme", selection: $store.preferences.theme) {
                    Text("System").tag("system"); Text("Light").tag("light"); Text("Dark").tag("dark")
                }.accessibilityIdentifier("appearance-theme")
                VStack(alignment: .leading, spacing: 12) {
                    LabeledContent("Message text size") { Text("\(Int(store.preferences.textSize)) pt").font(Theme.mono(13)).foregroundStyle(Theme.muted) }
                    HStack(spacing: 12) {
                        Text("A").font(.system(size: 14))
                        Slider(value: $store.preferences.textSize, in: 14...24, step: 1).accessibilityLabel("Message text size")
                        Text("A").font(.system(size: 24))
                    }.foregroundStyle(Theme.muted)
                }.padding(.vertical, 6)
            } header: { Eyebrow("Display") }.cardRows()
            Section {
                Toggle("Sender pictures in every chat", isOn: $store.preferences.senderPictures)
            } header: { Eyebrow("Chats") } footer: { Text("Group chats always show a picture beside each run of messages.") }.cardRows()
        }.themedForm().navigationTitle("Appearance").navigationBarTitleDisplayMode(.inline)
    }
}

private struct PrivacySettings: View {
    @Environment(ChatStore.self) private var store
    var body: some View {
        @Bindable var store = store
        List {
            Section {
                Toggle("Send read receipts", isOn: $store.sendReadReceipts)
                Toggle("Show when you are typing", isOn: $store.preferences.sendTyping)
                    .onChange(of: store.preferences.sendTyping) { _, enabled in if !enabled { store.stopComposing() } }
            } header: { Eyebrow("Privacy") } footer: { Text("Your WhatsApp privacy setting also applies to read and played receipts. Reading a chat updates its unread badge across devices.") }.cardRows()
            Section {
                Toggle("Prefer saved contact names", isOn: $store.preferences.contactNames)
                Toggle("Save contacts to phone address book", isOn: $store.preferences.saveContactsToPhone)
            } header: { Eyebrow("Contacts") }.cardRows()
        }.themedForm().navigationTitle("Privacy & contacts").navigationBarTitleDisplayMode(.inline)
    }
}

private struct MediaSettings: View {
    @Environment(ChatStore.self) private var store
    var body: some View {
        @Bindable var store = store
        List {
            Section {
                Toggle("Download attachments automatically", isOn: $store.preferences.autoDownload)
            } header: { Eyebrow("Downloads") } footer: { Text("Downloads visible attachments up to 64 MB while you read a conversation.") }.cardRows()
            Section {
                SecureField("GIPHY API key", text: $store.preferences.giphyKey).textInputAutocapitalization(.never).autocorrectionDisabled()
                    .font(Theme.mono(14))
                Link("Get a GIPHY API key", destination: URL(string: "https://developers.giphy.com/")!)
            } header: { Eyebrow("GIF search") } footer: { Text("Connect GIPHY to search and send GIFs from the attachment picker.") }.cardRows()
        }.themedForm().navigationTitle("Media & downloads").navigationBarTitleDisplayMode(.inline)
    }
}

private struct NotificationSettings: View {
    @Environment(ChatStore.self) private var store
    var body: some View {
        @Bindable var store = store
        List {
            Section {
                Toggle("Notify about new messages", isOn: $store.preferences.notifications)
                    .onChange(of: store.preferences.notifications) { _, enabled in
                        guard enabled, !store.isDemo else { return }
                        Task { if !(await store.notifications.request()) { store.preferences.notifications = false; store.error = "Enable notifications for Whatsapp in iPhone Settings." } }
                    }
            } header: { Eyebrow("Alerts") } footer: { Text("Alerts arrive while this app is connected: when open, briefly after you leave it, and during background refreshes iOS allows. Muted chats do not notify you.") }.cardRows()
        }.themedForm().navigationTitle("Notifications").navigationBarTitleDisplayMode(.inline)
    }
}

private struct AboutSettings: View {
    var body: some View {
        List {
            Section {
                HStack(spacing: 12) {
                    Image("Brand").resizable().frame(width: 44, height: 44).clipShape(RoundedRectangle(cornerRadius: 10))
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Whatsapp").font(Theme.sans(17, .semibold))
                        Text("Version \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.13.1") (\(Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "4"))")
                            .font(Theme.mono(12)).foregroundStyle(Theme.muted)
                    }
                }.padding(.vertical, 5)
                Link("Project and updates", destination: URL(string: "https://github.com/fightingentropy/Whatsapp")!)
            } header: { Eyebrow("App") } footer: { Text("An independent companion for your own iPhone. Messages and this device’s keys stay in the app’s private storage.") }.cardRows()
            Section {
                Text("Calls, status posts and group administration are not supported by this client.")
                Text("To unlink, remove “Whatsapp for iPhone” in the official app’s Linked Devices settings.")
            } header: { Eyebrow("Limits") }.font(Theme.sans(14)).foregroundStyle(Theme.muted).cardRows()
        }.themedForm().navigationTitle("About").navigationBarTitleDisplayMode(.inline)
    }
}
