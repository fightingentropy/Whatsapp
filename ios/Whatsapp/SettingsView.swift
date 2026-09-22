import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: ChatStore
    @State private var unlinkPresented = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if let id = store.accountID {
                        NavigationLink { ChatDetails(id: id) } label: { account }
                    } else { account }
                }
                Section {
                    NavigationLink { AppearanceSettings() } label: { SettingsLabel("Appearance", icon: "paintpalette") }
                    NavigationLink { PrivacySettings() } label: { SettingsLabel("Privacy & contacts", icon: "hand.raised") }
                    NavigationLink { MediaSettings() } label: { SettingsLabel("Media & downloads", icon: "photo.on.rectangle") }
                    NavigationLink { NotificationSettings() } label: { SettingsLabel("Notifications", icon: "bell") }
                }
                Section {
                    if !store.isDemo {
                        Button { store.reconnect() } label: { SettingsLabel("Reconnect", icon: "arrow.triangle.2.circlepath") }.tint(.primary)
                    }
                    NavigationLink { AboutSettings() } label: { SettingsLabel("About Whatsapp", icon: "info.circle") }
                } footer: { Text("Keep Whatsapp open to receive messages. It reconnects when you return.") }
                Section {
                    Button("Unlink this iPhone", role: .destructive) { unlinkPresented = true }
                }
            }
            .navigationTitle("Settings")
            .confirmationDialog("Unlink this iPhone and clear its local chat history?", isPresented: $unlinkPresented, titleVisibility: .visible) {
                Button("Unlink this iPhone", role: .destructive) { store.unlink() }
            } message: { Text("Your primary WhatsApp account and messages on other devices will stay intact.") }
        }
    }

    private var account: some View {
        HStack(spacing: 14) {
            AvatarView(name: store.accountName, url: store.accountID.flatMap { store.localURL(store.avatars[$0]) }, size: 56)
            VStack(alignment: .leading, spacing: 5) {
                Text(store.accountName).font(.headline)
                HStack(spacing: 5) {
                    Circle().fill(store.connected ? Color.accentColor : .secondary).frame(width: 5, height: 5)
                    Text(store.connectionLabel).font(.subheadline).foregroundStyle(.secondary)
                }
            }
        }.padding(.vertical, 8)
    }
}

private struct SettingsLabel: View {
    let title: String
    let icon: String
    init(_ title: String, icon: String) { self.title = title; self.icon = icon }
    var body: some View {
        Label { Text(title) } icon: { Image(systemName: icon).font(.system(size: 18, weight: .regular)).foregroundStyle(.secondary) }
            .padding(.vertical, 3)
    }
}

private struct AppearanceSettings: View {
    @EnvironmentObject private var store: ChatStore
    var body: some View {
        List {
            Section {
                VStack(alignment: .trailing, spacing: 3) {
                    Text("A little more room to read.").font(.system(size: store.preferences.textSize))
                    HStack(spacing: 4) { Text("10:24"); DeliveryMark(status: "read") }.font(.system(size: 10.5)).foregroundStyle(.secondary)
                }.padding(.horizontal, 12).padding(.vertical, 9)
                    .background(ChatAppearance.outgoing, in: RoundedRectangle(cornerRadius: 16))
                    .frame(maxWidth: .infinity, alignment: .trailing).padding(.vertical, 14)
                    .accessibilityLabel("Message text preview")
            }.listRowBackground(ChatAppearance.canvas)
            Section {
                Picker("Theme", selection: $store.preferences.theme) {
                    Text("System").tag("system"); Text("Light").tag("light"); Text("Dark").tag("dark")
                }.accessibilityIdentifier("appearance-theme")
                VStack(alignment: .leading, spacing: 12) {
                    LabeledContent("Message text size", value: "\(Int(store.preferences.textSize)) pt")
                    HStack(spacing: 12) {
                        Text("A").font(.system(size: 14))
                        Slider(value: $store.preferences.textSize, in: 14...24, step: 1).accessibilityLabel("Message text size")
                        Text("A").font(.system(size: 24))
                    }.foregroundStyle(.secondary)
                }.padding(.vertical, 6)
            }
            Section {
                Toggle("Sender pictures in every chat", isOn: $store.preferences.senderPictures)
            } footer: { Text("Group chats always show a picture beside each run of messages.") }
        }.navigationTitle("Appearance").navigationBarTitleDisplayMode(.inline)
    }
}

private struct PrivacySettings: View {
    @EnvironmentObject private var store: ChatStore
    var body: some View {
        List {
            Section {
                Toggle("Send read receipts", isOn: $store.sendReadReceipts)
                Toggle("Show when you are typing", isOn: $store.preferences.sendTyping)
                    .onChange(of: store.preferences.sendTyping) { _, enabled in if !enabled { store.stopComposing() } }
            } footer: { Text("Your WhatsApp privacy setting also applies to read and played receipts. Reading a chat updates its unread badge across devices.") }
            Section("Contacts") {
                Toggle("Prefer saved contact names", isOn: $store.preferences.contactNames)
                Toggle("Save contacts to phone address book", isOn: $store.preferences.saveContactsToPhone)
            }
        }.navigationTitle("Privacy & contacts").navigationBarTitleDisplayMode(.inline)
    }
}

private struct MediaSettings: View {
    @EnvironmentObject private var store: ChatStore
    var body: some View {
        List {
            Section {
                Toggle("Download attachments automatically", isOn: $store.preferences.autoDownload)
            } footer: { Text("Downloads visible attachments up to 64 MB while you read a conversation.") }
            Section {
                SecureField("GIPHY API key", text: $store.preferences.giphyKey).textInputAutocapitalization(.never).autocorrectionDisabled()
                Link("Get a GIPHY API key", destination: URL(string: "https://developers.giphy.com/")!)
            } header: { Text("GIF search") } footer: { Text("Connect GIPHY to search and send GIFs from the attachment picker.") }
        }.navigationTitle("Media & downloads").navigationBarTitleDisplayMode(.inline)
    }
}

private struct NotificationSettings: View {
    @EnvironmentObject private var store: ChatStore
    var body: some View {
        List {
            Section {
                Toggle("Notify about new messages", isOn: $store.preferences.notifications)
                    .onChange(of: store.preferences.notifications) { _, enabled in
                        guard enabled, !store.isDemo else { return }
                        Task { if !(await store.notifications.request()) { store.preferences.notifications = false; store.error = "Enable notifications for Whatsapp in iPhone Settings." } }
                    }
            } footer: { Text("Alerts work while this app is connected, including its brief background allowance. They stop when iOS suspends the app. Muted chats do not notify you.") }
        }.navigationTitle("Notifications").navigationBarTitleDisplayMode(.inline)
    }
}

private struct AboutSettings: View {
    var body: some View {
        List {
            Section {
                HStack(spacing: 12) {
                    Image("Brand").resizable().frame(width: 44, height: 44).clipShape(RoundedRectangle(cornerRadius: 10))
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Whatsapp").font(.headline)
                        Text("Version \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.13.1") (\(Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "4"))")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }.padding(.vertical, 5)
                Link("Project and updates", destination: URL(string: "https://github.com/fightingentropy/Whatsapp")!)
            } footer: { Text("An independent companion for your own iPhone. Messages and this device’s keys stay in the app’s private storage.") }
            Section {
                Text("Calls, status posts and group administration are not supported by this client.")
                Text("To unlink, remove “Whatsapp for iPhone” in the official app’s Linked Devices settings.")
            }.font(.subheadline).foregroundStyle(.secondary)
        }.navigationTitle("About").navigationBarTitleDisplayMode(.inline)
    }
}
