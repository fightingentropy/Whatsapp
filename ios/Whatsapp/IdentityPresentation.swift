import Foundation
import Observation

/// A row subscribes to the identity it displays, rather than every dictionary key.
@MainActor @Observable
final class IdentityPresentation {
    var contact: CoreEvent.Contact?
    var savedName: String?
    var chatName: String?
    var avatar: String?
    var fullAvatar: String?
    var revision: String?
    var typing: [String: Date] = [:]
    var online = false
    var lastSeen: TimeInterval?
}

extension ChatStore {
    func applyPortraits(_ events: [CoreEvent]) {
        // Copy/compare dictionaries once per restored slice, not once per face.
        var previews = avatars, full = fullAvatars, revisions = avatarRevisions
        for event in events {
            guard let id = event.id.map(canonical) else { continue }
            if event.full == true { full[id] = event.path } else { previews[id] = event.path }
            revisions[id] = event.revision ?? event.path.map { MediaFiles.inspect(URL(fileURLWithPath: $0)) }
        }
        if avatars != previews { avatars = previews }
        if fullAvatars != full { fullAvatars = full }
        if avatarRevisions != revisions { avatarRevisions = revisions }
    }

    func mergeIdentity(from: String, into: String) {
        let source = canonical(from), target = canonical(into)
        guard source != target else { return }
        if let value = avatars.removeValue(forKey: source), avatars[target] == nil { avatars[target] = value }
        if let value = fullAvatars.removeValue(forKey: source), fullAvatars[target] == nil { fullAvatars[target] = value }
        if let value = avatarRevisions.removeValue(forKey: source), avatarRevisions[target] == nil { avatarRevisions[target] = value }
        if let value = contactNames.removeValue(forKey: source), contactNames[target] == nil { contactNames[target] = value }
        if let value = typing.removeValue(forKey: source), typing[target] == nil { typing[target] = value }
        if let value = presence.removeValue(forKey: source), presence[target] == nil { presence[target] = value }
    }

    func identity(_ id: String) -> IdentityPresentation {
        if let value = identities[id] { return value }
        let value = IdentityPresentation()
        identities[id] = value
        return value
    }

    func indexContacts() {
        var indexed: [String: CoreEvent.Contact] = [:]
        // Prefer a canonical contact over a stale alias, independent of arrival order.
        for contact in contacts {
            let id = canonical(contact.id)
            if indexed[id] == nil || contact.id == id { indexed[id] = contact }
        }
        for id in Set(contactIDs).union(indexed.keys) {
            let state = identity(id)
            if state.contact != indexed[id] { state.contact = indexed[id] }
        }
        contactIDs = Set(indexed.keys)
    }

    func indexChats() {
        let indexed = Dictionary(chats.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        for id in Set(chatsByID.keys).union(indexed.keys) {
            let state = identity(id)
            if state.chatName != indexed[id]?.name { state.chatName = indexed[id]?.name }
        }
        chatsByID = indexed
    }

    func syncIdentityValues<Value: Equatable>(_ old: [String: Value], _ new: [String: Value],
                                             update: (IdentityPresentation, Value?) -> Void) {
        for id in Set(old.keys).union(new.keys) where old[id] != new[id] {
            update(identity(canonical(id)), new[id])
        }
    }

    func isTyping(in chat: String) -> Bool { !identity(canonical(chat)).typing.isEmpty }

    func chatByID(_ id: String) -> Chat? {
        // The collection supplies observation; the index supplies constant-time lookup.
        _ = chats
        return chatsByID[canonical(id)]
    }
}
