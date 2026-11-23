import Contacts
import SwiftUI

struct ChatDetails: View {
    let id: String
    @Environment(ChatStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var editingName = false
    @State private var name = ""
    private var chat: Chat? { store.chatByID(id) }
    var body: some View {
        List {
            Section {
                VStack(spacing: 7) {
                    AvatarView(name: chat.map(store.chatTitle) ?? store.displayName(id), url: store.avatarURL(id, full: true), group: chat?.kind == "group", size: 88)
                        .padding(.bottom, 8)
                    Text(chat.map(store.chatTitle) ?? store.displayName(id)).font(Theme.sans(24, .bold)).tracking(-0.6).foregroundStyle(Theme.fg).textSelection(.enabled)
                    if id.hasSuffix("@s.whatsapp.net") { Text("+" + id.components(separatedBy: "@")[0]).font(Theme.mono(14)).foregroundStyle(Theme.muted).textSelection(.enabled) }
                    if let chat, let status = store.presenceLabel(chat) { Text(status).font(Theme.mono(12.5)).foregroundStyle(Theme.muted) }
                    if id == store.accountID, let about = store.accountAbout { Text(about).font(Theme.sans(14)).foregroundStyle(Theme.muted) }
                }.frame(maxWidth: .infinity).multilineTextAlignment(.center).padding(.vertical, 12)
            }.listRowBackground(Color.clear)
            if let chat {
                Section {
                    Button { store.setPinned(chat) } label: { Label(chat.pinned ? "Unpin chat" : "Pin chat", systemImage: chat.pinned ? "pin.slash" : "pin") }
                    Button { store.setArchived(chat) } label: { Label(chat.archived ? "Unarchive chat" : "Archive chat", systemImage: "archivebox") }
                    ChatMuteMenu(chat: chat)
                    Button { store.markChatRead(chat) } label: { Label("Mark as read", systemImage: "checkmark.bubble") }
                } header: { Eyebrow("Chat") }.tint(Theme.fg).cardRows()
                if chat.kind == "group" {
                    Section {
                        ForEach(chat.participants ?? [], id: \.self) { member in
                            NavigationLink { ChatDetails(id: member) } label: {
                                HStack(spacing: 12) {
                                    AvatarView(name: store.displayName(member), url: store.avatarURL(member), size: 36)
                                    Text(store.displayName(member))
                                }.padding(.vertical, 3)
                            }.task { store.avatar(member) }
                        }
                        if chat.participants?.isEmpty != false { Text("Member information is still loading.").foregroundStyle(Theme.muted) }
                    } header: { Eyebrow("Members") }.cardRows()
                }
            }
            if chat?.kind != "group" && id != store.accountID {
                Section {
                    Button("Message") {
                        store.ensureChat(id); dismiss()
                    }
                    Button("Save contact name") { name = store.displayName(id); editingName = true }
                } header: { Eyebrow("Contact") }.cardRows()
            }
        }
        .themedForm()
        .navigationTitle(chat?.kind == "group" ? "Group info" : "Contact info")
        .navigationBarTitleDisplayMode(.inline)
        .task { store.fetchFullAvatar(id) }
        .alert("Save contact", isPresented: $editingName) {
            TextField("Full name", text: $name)
            Button("Save") { store.saveContact(id: id, name: name) }
            Button("Cancel", role: .cancel) {}
        } message: { Text(store.preferences.saveContactsToPhone ? "The name will also be saved to your phone’s address book through WhatsApp." : "The name will sync as a WhatsApp contact.") }
    }
}

struct ChatMuteMenu: View {
    let chat: Chat
    @Environment(ChatStore.self) private var store
    var body: some View {
        Menu {
            if chat.muted { Button("Unmute") { store.setMuted(chat, seconds: nil) } }
            Button("8 hours") { store.setMuted(chat, seconds: 28_800) }
            Button("1 week") { store.setMuted(chat, seconds: 604_800) }
            Button("Always") { store.setMuted(chat, seconds: 0) }
        } label: { Label(chat.muted ? "Muted" : "Mute notifications", systemImage: chat.muted ? "bell.slash.fill" : "bell") }
    }
}

struct NewChatView: View {
    @Environment(ChatStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var phone = ""
    @State private var name = ""
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Phone number with country code", text: $phone).keyboardType(.phonePad).accessibilityIdentifier("new-contact-phone")
                    TextField("Save name (optional)", text: $name).textContentType(.name)
                    Button { store.newContact(phone: phone, name: name) } label: {
                        if store.newContactBusy { ProgressView() } else { Text("Continue") }
                    }.disabled(store.newContactBusy || phone.isEmpty).accessibilityIdentifier("new-contact-submit")
                } header: { Eyebrow("New conversation") }.cardRows()
                Section {
                    ForEach(store.contacts.filter { !$0.id.hasSuffix("@g.us") }.sorted { ($0.name ?? "") < ($1.name ?? "") }, id: \.id) { contact in
                        Button(store.displayName(contact.id)) {
                            store.ensureChat(contact.id); dismiss()
                        }
                    }
                } header: { Eyebrow("Contacts") }.cardRows()
            }.themedForm().navigationTitle("New chat")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
                .onChange(of: store.navigation) { _, _ in if !store.navigation.isEmpty { dismiss() } }
        }
    }
}

struct MessageDetails: View {
    let message: Message
    @Environment(ChatStore.self) private var store
    var body: some View {
        List {
            Section {
                LabeledContent("From", value: store.displayName(message.sender, fallback: message.senderName))
                LabeledContent("Sent", value: Date(timeIntervalSince1970: message.timestamp).formatted())
                LabeledContent("Status", value: message.status.capitalized)
                if let time = message.deliveredAt { LabeledContent("Delivered", value: Date(timeIntervalSince1970: time).formatted()) }
                if let time = message.readAt { LabeledContent("Read / played", value: Date(timeIntervalSince1970: time).formatted()) }
                if message.forwarded == true { Label("Forwarded", systemImage: "arrowshape.turn.up.right") }
                if message.edited { Label("Edited", systemImage: "pencil") }
            } header: { Eyebrow("Delivery") }.cardRows()
            if let reactions = message.reactionDetails, !reactions.isEmpty {
                Section { ForEach(Array(reactions.enumerated()), id: \.offset) { _, reaction in LabeledContent(store.displayName(reaction.sender), value: reaction.emoji) } } header: { Eyebrow("Reactions") }.cardRows()
            }
            Section { Text(message.id).font(Theme.mono(12)).foregroundStyle(Theme.muted).textSelection(.enabled) } header: { Eyebrow("Message ID") }.cardRows()
        }.themedForm().navigationTitle("Message info").navigationBarTitleDisplayMode(.inline)
    }
}

struct ForwardPicker: View {
    let message: Message
    @Environment(ChatStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var target: Chat?
    @State private var sending = false
    var body: some View {
        NavigationStack {
            List(store.chats.filter { !$0.readOnly && (query.isEmpty || $0.name.localizedStandardContains(query)) }) { chat in
                Button { target = chat } label: {
                    HStack { Text(chat.name).foregroundStyle(Theme.fg); Spacer(); if target?.id == chat.id { Image(systemName: "checkmark.circle.fill").foregroundStyle(Theme.green) } }
                }.cardRows()
            }
            .themedForm()
            .searchable(text: $query).navigationTitle("Forward message")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Forward") {
                        guard let target else { return }; sending = true
                        store.forward(message, to: target) { accepted in sending = false; if accepted { dismiss() } }
                    }.disabled(target == nil || sending || (!store.connected && !store.isDemo))
                }
            }
        }
    }
}

struct ContactCard: View {
    let vcard: String
    @Environment(ChatStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    private var contact: CNContact? { try? CNContactVCardSerialization.contacts(with: Data(vcard.utf8)).first }
    var body: some View {
        List {
            if let contact {
                Text(CNContactFormatter.string(from: contact, style: .fullName) ?? "Contact").font(.title2)
                ForEach(contact.phoneNumbers, id: \.identifier) { phone in
                    VStack(alignment: .leading) {
                        Text(phone.value.stringValue).textSelection(.enabled)
                        Button("Message on WhatsApp") { store.newContact(phone: phone.value.stringValue, name: "") }
                    }
                }
                ForEach(contact.emailAddresses, id: \.identifier) { email in Text(email.value as String).textSelection(.enabled) }
            } else { Text(vcard).font(Theme.mono(12)).textSelection(.enabled) }
        }.themedForm().navigationTitle("Contact card")
            .onChange(of: store.navigation) { _, _ in dismiss() }
    }
}
