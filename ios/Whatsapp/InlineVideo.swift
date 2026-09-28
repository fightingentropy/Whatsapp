import AVKit
import Observation
import SwiftUI

/// One native decoder and audio clock for the conversation, created by Play.
@MainActor @Observable
final class NativeVideo {
    private(set) var id: String?
    private(set) var player: AVPlayer?
    private(set) var failedID: String?
    @ObservationIgnored private var lease: UUID?
    @ObservationIgnored private var status: NSKeyValueObservation?

    func play(_ url: URL, id: String) async {
        stop()
        let token = UUID(); lease = token
        do { try await AudioSessionControl.acquire(token, recording: false, video: true) }
        catch { if lease == token { stop(); failedID = id }; return }
        guard lease == token else { AudioSessionControl.release(token); return }
        let item = AVPlayerItem(url: url)
        let player = AVPlayer(playerItem: item)
        self.id = id; self.player = player
        status = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            Task { @MainActor in
                guard self?.lease == token else { return }
                self?.stop(); self?.failedID = id
            }
        }
        player.play()
    }

    func pause(_ id: String) { if self.id == id { player?.pause() } }
    func stop() {
        status = nil
        player?.pause(); player?.replaceCurrentItem(with: nil)
        player = nil; id = nil; failedID = nil
        AudioSessionControl.release(lease); lease = nil
    }
}

struct InlineVideo: View {
    let message: Message
    @Environment(ChatStore.self) private var store

    private var size: CGSize {
        let width = Double(message.content?.media?.width ?? 16)
        let height = Double(message.content?.media?.height ?? 9)
        let scale = min(240 / max(1, width), 280 / max(1, height))
        return CGSize(width: max(100, width * scale), height: max(80, height * scale))
    }

    var body: some View {
        Group {
            if store.video.id == message.id, let player = store.video.player {
                VideoPlayer(player: player).accessibilityLabel("Video player").accessibilityIdentifier("inline-video-\(message.id)")
            } else {
                Button {
                    guard let url = store.localURL(message.mediaPath) else { store.download(message); return }
                    guard !store.audio.hasRecording else { store.error = "Finish or discard the voice recording first."; return }
                    store.audioRequest = UUID(); store.audio.stopPlayback()
                    Task {
                        guard store.selectedChat == store.canonical(message.chat), store.foreground || store.isDemo else { return }
                        await store.video.play(url, id: message.id)
                    }
                } label: {
                    ZStack {
                        Color.black
                        if let url = store.localURL(message.mediaPath) {
                            LocalImage(url: url, maximumSize: 640).scaledToFit()
                        }
                        if message.mediaState == "downloading" { ProgressView().tint(.white) }
                        else {
                            Image(systemName: message.mediaPath == nil ? "arrow.down" : "play.fill")
                                .font(.title).foregroundStyle(.white).padding(18).background(.black.opacity(0.55), in: Circle())
                        }
                        VStack { Spacer(); HStack {
                            Text(store.video.failedID == message.id ? "Could not play · retry" : (message.mediaError ?? Duration.seconds(message.content?.seconds ?? 0).formatted(.time(pattern: .minuteSecond))))
                                .font(.caption).foregroundStyle(.white).padding(5).background(.black.opacity(0.65), in: Capsule())
                            Spacer()
                        }.padding(7) }
                    }
                }.buttonStyle(.plain)
                    .accessibilityLabel(message.mediaPath == nil ? "Download video" : "Play video")
                    .accessibilityIdentifier("play-video-\(message.id)")
                    .disabled(message.mediaState == "downloading" || (message.mediaPath == nil && !store.connected))
            }
        }
        .frame(width: size.width, height: size.height)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .onScrollVisibilityChange(threshold: 0.1) { visible in if !visible { store.video.pause(message.id) } }
        .onDisappear { store.video.pause(message.id) }
    }
}
