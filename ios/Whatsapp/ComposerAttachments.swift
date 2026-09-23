import SwiftUI

enum ComposerAttachment: String, Identifiable {
    case photos, camera, document, paste, emoji, gifs, stickers, mention
    var id: String { rawValue }
    static func actions(group: Bool) -> [Self] {
        [.photos, .camera, .document, .paste, .emoji, .gifs, .stickers] + (group ? [.mention] : [])
    }
    var title: String {
        switch self {
        case .photos: "Photos"
        case .camera: "Camera"
        case .document: "Document"
        case .paste: "Paste photo"
        case .emoji: "Emoji"
        case .gifs: "GIFs"
        case .stickers: "Stickers"
        case .mention: "Mention"
        }
    }
    var symbol: String {
        switch self {
        case .photos: "photo.on.rectangle.fill"
        case .camera: "camera.fill"
        case .document: "doc.fill"
        case .paste: "doc.on.clipboard.fill"
        case .emoji: "face.smiling.fill"
        case .gifs: "play.rectangle.fill"
        case .stickers: ""
        case .mention: "at"
        }
    }
    var color: Color {
        switch self {
        case .photos: .blue
        case .camera: .primary
        case .document: .cyan
        case .paste: .purple
        case .emoji: .yellow
        case .gifs: .mint
        case .stickers: .pink
        case .mention: .orange
        }
    }
}

struct ComposerAttachments: View {
    let group: Bool
    let select: (ComposerAttachment) -> Void
    @ScaledMetric(relativeTo: .subheadline) private var labelSize = 14

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8, alignment: .top), count: 4), spacing: 28) {
            ForEach(ComposerAttachment.actions(group: group)) { item in
                Button { select(item) } label: {
                    VStack(spacing: 10) {
                        ZStack {
                            Circle().fill(ChatAppearance.attachmentCircle).frame(width: 60, height: 60)
                            if item == .stickers {
                                ComposerStickerIcon().stroke(item.color, style: StrokeStyle(lineWidth: 2.4, lineCap: .round, lineJoin: .round)).frame(width: 29, height: 29)
                            } else { Image(systemName: item.symbol).font(.system(size: 29, weight: .medium)).foregroundStyle(item.color) }
                        }
                        Text(item.title).font(.system(size: labelSize)).foregroundStyle(.primary).lineLimit(2).multilineTextAlignment(.center)
                    }.frame(maxWidth: .infinity, alignment: .top).contentShape(Rectangle())
                }.buttonStyle(.plain).accessibilityIdentifier("attachment-" + item.rawValue)
            }
        }
        .padding(.horizontal, 18).padding(.top, 30).padding(.bottom, 24)
        .background {
            RoundedRectangle(cornerRadius: 30).fill(ChatAppearance.attachmentPanel).ignoresSafeArea(.container, edges: .bottom)
        }
        .overlay {
            RoundedRectangle(cornerRadius: 30).stroke(.primary.opacity(0.06), lineWidth: 0.5).ignoresSafeArea(.container, edges: .bottom)
        }
        .accessibilityElement(children: .contain).accessibilityIdentifier("composer-attachment-panel")
    }
}

/// The outlined, folded sticker used inside the reference's text field.
struct ComposerStickerIcon: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: 12, y: 2))
        path.addCurve(to: CGPoint(x: 22, y: 12), control1: CGPoint(x: 20, y: 2), control2: CGPoint(x: 22, y: 4))
        path.addCurve(to: CGPoint(x: 12, y: 22), control1: CGPoint(x: 22, y: 17), control2: CGPoint(x: 17, y: 22))
        path.addCurve(to: CGPoint(x: 2, y: 12), control1: CGPoint(x: 4, y: 22), control2: CGPoint(x: 2, y: 20))
        path.addCurve(to: CGPoint(x: 12, y: 2), control1: CGPoint(x: 2, y: 4), control2: CGPoint(x: 4, y: 2))
        path.closeSubpath()
        path.move(to: CGPoint(x: 12, y: 22))
        path.addCurve(to: CGPoint(x: 22, y: 12), control1: CGPoint(x: 12, y: 14), control2: CGPoint(x: 14, y: 12))
        return path.applying(CGAffineTransform(scaleX: rect.width / 24, y: rect.height / 24).concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY)))
    }
}
