import Foundation

enum OrderedUpdates {
    /// Keep the existing order and sort only the changed portion. The fallback
    /// also accepts an unsorted or duplicated initial snapshot (last ID wins).
    static func merge<Value: Identifiable>(_ existing: [Value], _ incoming: [Value],
                                           by precedes: (Value, Value) -> Bool) -> [Value] {
        guard !incoming.isEmpty else { return existing }
        let updates = Dictionary(incoming.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        var seen = Set<Value.ID>()
        seen.reserveCapacity(existing.count)
        var retained: [Value] = []
        retained.reserveCapacity(existing.count)
        for value in existing.reversed() where updates[value.id] == nil && seen.insert(value.id).inserted {
            retained.append(value)
        }
        retained.reverse()
        if zip(retained, retained.dropFirst()).contains(where: { precedes($1, $0) }) {
            retained.sort(by: precedes)
        }
        let changed = updates.values.sorted(by: precedes)
        var merged: [Value] = []
        merged.reserveCapacity(retained.count + changed.count)
        var index = 0
        for value in retained {
            while index < changed.count && precedes(changed[index], value) {
                merged.append(changed[index]); index += 1
            }
            merged.append(value)
        }
        merged.append(contentsOf: changed[index...])
        return merged
    }
}

extension CoreEvent {
    /// Only combine adjacent, independent collection updates. Requested pages,
    /// aliases, deletions, receipts and lifecycle events are ordering barriers.
    static func coalescing(_ events: [CoreEvent]) -> [CoreEvent] {
        var result: [CoreEvent] = []
        var index = 0
        while index < events.count {
            var event = events[index]; index += 1
            while index < events.count && events[index].type == event.type {
                let next = events[index]
                if event.type == "chats" {
                    if event.chats == nil { event.chats = [] }
                    event.chats?.append(contentsOf: next.chats ?? [])
                } else if event.type == "contacts" {
                    if event.contacts == nil { event.contacts = [] }
                    event.contacts?.append(contentsOf: next.contacts ?? [])
                } else if event.type == "messages", event.requested != true, next.requested != true, event.chat == next.chat {
                    if event.messages == nil { event.messages = [] }
                    event.messages?.append(contentsOf: next.messages ?? [])
                } else { break }
                index += 1
            }
            result.append(event)
        }
        return result
    }
}
