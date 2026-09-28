import SwiftUI

struct OutgoingAttachmentRow: View {
    let item: OutgoingAttachment
    @Environment(ChatStore.self) private var store
    var body: some View {
        HStack {
            Spacer(minLength: 34)
            VStack(alignment: .leading, spacing: 8) {
                Label(item.file.name, systemImage: "doc").font(.subheadline.weight(.medium)).lineLimit(2)
                if !item.caption.isEmpty { Text(item.caption).font(.subheadline).lineLimit(3) }
                if item.state == .uploading { ProgressView("Uploading…").font(.caption) }
                else if item.state == .queued { Label("Waiting to upload", systemImage: "clock").font(.caption).foregroundStyle(.secondary) }
                else { Text(item.error ?? "Upload failed").font(.caption).foregroundStyle(.secondary) }
                if item.state != .uploading {
                    HStack {
                        if item.state == .failed { Button("Retry") { store.retryAttachment(item) }.disabled(!store.connected) }
                        Button("Remove", role: .destructive) { store.discardAttachment(item) }
                    }.font(.subheadline).buttonStyle(.borderless)
                }
            }.padding(12).background(ChatAppearance.outgoing, in: RoundedRectangle(cornerRadius: 14))
        }.accessibilityIdentifier("upload-" + item.id)
    }
}
