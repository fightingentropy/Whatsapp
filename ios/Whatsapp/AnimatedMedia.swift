import AVFoundation
import ImageIO
import SwiftUI

struct AnimatedMedia: View {
    let url: URL
    var video = false
    @State private var visible = false
    @State private var image: UIImage?
    @Environment(\.scenePhase) private var phase
    var body: some View {
        Group {
            if video { LoopingVideo(url: url, playing: visible && phase == .active) }
            else if let image { AnimatedImage(image: image, playing: visible && phase == .active) }
            else { LocalImage(url: url, maximumSize: 360).scaledToFit() }
        }
        .onAppear { visible = true }
        .onDisappear { visible = false; image = nil }
        .task(id: visible) {
            guard visible, !video else { return }
            let decoder = Task.detached(priority: .utility) { Self.decode(url) }
            let result = await withTaskCancellationHandler { await decoder.value } onCancel: { decoder.cancel() }
            if !Task.isCancelled { image = result }
        }
    }

    nonisolated private static func decode(_ url: URL) -> UIImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary) else { return nil }
        let count = CGImageSourceGetCount(source)
        guard count > 1, count <= 10_000 else { return nil }
        // Keep visible animated stickers within 24 MiB, including long packs.
        let step = max(1, Int(ceil(Double(count) / 120)))
        let edge = min(360, Int(sqrt(Double(24 * 1024 * 1024) / Double(min(count, 120) * 4))))
        var frames: [UIImage] = []
        var duration = 0.0
        for index in 0..<count {
            guard !Task.isCancelled else { return nil }
            let props = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [String: Any] ?? [:]
            let timing = (props["{GIF}"] ?? props["{WebP}"] ?? props["{PNG}"]) as? [String: Any] ?? [:]
            duration += max(0.02, timing["UnclampedDelayTime"] as? Double ?? timing["DelayTime"] as? Double ?? 0.1)
            guard index % step == 0 else { continue }
            if let cg = CGImageSourceCreateThumbnailAtIndex(source, index, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: edge, kCGImageSourceShouldCacheImmediately: true] as CFDictionary) {
                frames.append(UIImage(cgImage: cg))
            }
        }
        return UIImage.animatedImage(with: frames, duration: duration)
    }
}

private struct AnimatedImage: UIViewRepresentable {
    let image: UIImage
    let playing: Bool
    func makeUIView(context: Context) -> UIImageView {
        let view = UIImageView(); view.contentMode = .scaleAspectFit; view.clipsToBounds = true
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        return view
    }
    func updateUIView(_ view: UIImageView, context: Context) {
        if view.image !== image { view.image = image }
        if playing { view.startAnimating() } else { view.stopAnimating() }
    }
    static func dismantleUIView(_ view: UIImageView, coordinator: ()) { view.stopAnimating(); view.image = nil }
}

private struct LoopingVideo: UIViewRepresentable {
    let url: URL
    let playing: Bool
    final class Surface: UIView {
        override class var layerClass: AnyClass { AVPlayerLayer.self }
        var playerLayer: AVPlayerLayer { layer as! AVPlayerLayer }
        var player: AVQueuePlayer?
        var loop: AVPlayerLooper?
    }
    func makeUIView(context: Context) -> Surface {
        let view = Surface()
        let player = AVQueuePlayer(); player.isMuted = true
        view.player = player; view.playerLayer.player = player; view.playerLayer.videoGravity = .resizeAspect
        view.loop = AVPlayerLooper(player: player, templateItem: AVPlayerItem(url: url))
        return view
    }
    func updateUIView(_ view: Surface, context: Context) { if playing { view.player?.play() } else { view.player?.pause() } }
    static func dismantleUIView(_ view: Surface, coordinator: ()) { view.player?.pause(); view.loop?.disableLooping(); view.player?.removeAllItems() }
}
