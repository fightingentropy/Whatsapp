import Contacts
import SwiftUI

struct ChatDetails: View {
    let id: String
    @EnvironmentObject private var store: ChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var editingName = false
    @State private var name = ""
    private var chat: Chat? { store.chats.first { $0.id == store.canonical(id) } }
    var body: some View {
        List {
            Section {
                VStack(spacing: 12) {
                    AvatarView(name: store.displayName(id), url: store.localURL(store.fullAvatars[store.canonical(id)] ?? store.avatars[store.canonical(id)]), group: chat?.kind == "group")
                        .scaleEffect(1.5).padding(20)
                    Text(chat.map(store.chatTitle) ?? store.displayName(id)).font(.title2.bold()).textSelection(.enabled)
                    if id.hasSuffix("@s.whatsapp.net") { Text("+" + id.components(separatedBy: "@")[0]).foregroundStyle(.secondary).textSelection(.enabled) }
                    if let chat, let status = store.presenceLabel(chat) { Text(status).font(.footnote).foregroundStyle(.secondary) }
                    if id == store.accountID, let about = store.accountAbout { Text(about).foregroundStyle(.secondary) }
                }.frame(maxWidth: .infinity)
            }
            if let chat {
                Section {
                    Button(chat.pinned ? "Unpin chat" : "Pin chat") { store.setPinned(chat) }
                    Button(chat.archived ? "Unarchive chat" : "Archive chat") { store.setArchived(chat) }
                    ChatMuteMenu(chat: chat)
                    Button("Mark as read") { store.markChatRead(chat) }
                }
                if chat.kind == "group" {
                    Section("Members") {
                        ForEach(chat.participants ?? [], id: \.self) { member in
                            NavigationLink { ChatDetails(id: member) } label: {
                                HStack { AvatarView(name: store.displayName(member), url: store.localURL(store.avatars[member])); Text(store.displayName(member)) }
                            }.task { store.avatar(member) }
                        }
                        if chat.participants?.isEmpty != false { Text("Member information is still loading.").foregroundStyle(.secondary) }
                    }
                }
            }
            if chat?.kind != "group" && id != store.accountID {
                Section {
                    Button("Message") {
                        store.ensureChat(id); dismiss()
                    }
                    Button("Save contact name") { name = store.displayName(id); editingName = true }
                }
            }
        }
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
    @EnvironmentObject private var store: ChatStore
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
    @EnvironmentObject private var store: ChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var phone = ""
    @State private var name = ""
    var body: some View {
        NavigationStack {
            Form {
                Section("New conversation") {
                    TextField("Phone number with country code", text: $phone).keyboardType(.phonePad).accessibilityIdentifier("new-contact-phone")
                    TextField("Save name (optional)", text: $name).textContentType(.name)
                    Button { store.newContact(phone: phone, name: name) } label: {
                        if store.newContactBusy { ProgressView() } else { Text("Continue") }
                    }.disabled(store.newContactBusy || phone.isEmpty).accessibilityIdentifier("new-contact-submit")
                }
                Section("Contacts") {
                    ForEach(store.contacts.filter { !$0.id.hasSuffix("@g.us") }.sorted { ($0.name ?? "") < ($1.name ?? "") }, id: \.id) { contact in
                        Button(store.displayName(contact.id)) {
                            store.ensureChat(contact.id); dismiss()
                        }
                    }
                }
            }.navigationTitle("New chat")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
                .onChange(of: store.navigation) { _, _ in if !store.navigation.isEmpty { dismiss() } }
        }
    }
}

struct MessageDetails: View {
    let message: Message
    @EnvironmentObject private var store: ChatStore
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
            }
            if let reactions = message.reactionDetails, !reactions.isEmpty {
                Section("Reactions") { ForEach(Array(reactions.enumerated()), id: \.offset) { _, reaction in LabeledContent(store.displayName(reaction.sender), value: reaction.emoji) } }
            }
            Section { Text(message.id).font(.caption.monospaced()).textSelection(.enabled) } header: { Text("Message ID") }
        }.navigationTitle("Message info").navigationBarTitleDisplayMode(.inline)
    }
}

struct ForwardPicker: View {
    let message: Message
    @EnvironmentObject private var store: ChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var target: Chat?
    @State private var sending = false
    var body: some View {
        NavigationStack {
            List(store.chats.filter { !$0.readOnly && (query.isEmpty || $0.name.localizedStandardContains(query)) }) { chat in
                Button { target = chat } label: {
                    HStack { Text(chat.name).foregroundStyle(.primary); Spacer(); if target?.id == chat.id { Image(systemName: "checkmark.circle.fill") } }
                }
            }
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
    @EnvironmentObject private var store: ChatStore
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
            } else { Text(vcard).font(.footnote).textSelection(.enabled) }
        }.navigationTitle("Contact card")
            .onChange(of: store.navigation) { _, _ in dismiss() }
    }
}
