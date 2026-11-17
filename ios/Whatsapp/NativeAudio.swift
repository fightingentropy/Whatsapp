import AVFoundation
import Observation
import SwiftUI

// Audio route negotiation can block. Keep it off SwiftUI's main actor and
// scope releases so a late cancellation cannot silence a newer recording.
enum AudioSessionControl {
    static let queue = DispatchQueue(label: "org.erlin.whatsapp.ios.audio-session", qos: .userInitiated)
    private static var owner: UUID?
    static func acquire(_ token: UUID, recording: Bool, video: Bool = false) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            queue.async {
                do {
                    let session = AVAudioSession.sharedInstance()
                    try session.setCategory(recording ? .playAndRecord : .playback, mode: recording ? .voiceChat : (video ? .moviePlayback : .spokenAudio), options: recording ? [.defaultToSpeaker] : [])
                    try session.setActive(true)
                    owner = token
                    continuation.resume()
                } catch { continuation.resume(throwing: error) }
            }
        }
    }
    static func release(_ token: UUID?) {
        guard let token else { return }
        queue.async {
            guard owner == token else { return }
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            owner = nil
        }
    }
}

private final class PreparedAudio: @unchecked Sendable {
    let player: AVAudioPlayer
    init(url: URL) throws { player = try AVAudioPlayer(contentsOf: url); player.prepareToPlay() }
}

/// Property-scoped observation: the 10 Hz position and level updates only
/// invalidate the views that read them, not the composer or every voice row.
@MainActor @Observable
final class NativeAudio: NSObject, AVAudioPlayerDelegate {
    var playingID: String?
    var position = 0.0
    var duration = 0.0
    private(set) var isPlaying = false
    var isRecording = false
    var hasRecording = false
    var recordingChat: String?
    @ObservationIgnored private(set) var recordingID: UUID?
    @ObservationIgnored var quotedMessageID: String?
    var recordingDuration = 0.0
    var levels: [Double] = []
    var error: String?
    @ObservationIgnored private var player: AVAudioPlayer?
    @ObservationIgnored private var recorder: AVAudioRecorder?
    @ObservationIgnored private var recordingURL: URL?
    @ObservationIgnored private var pcmURL: URL?
    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private var requestingPermission = false
    @ObservationIgnored private var audioLease: UUID?

    func play(url: URL, id: String) async throws -> Bool {
        guard !hasRecording else { return false }
        if playingID == id, let player {
            if player.isPlaying { player.pause(); timer?.invalidate(); timer = nil }
            else {
                let token = UUID(); audioLease = token
                try await AudioSessionControl.acquire(token, recording: false)
                guard audioLease == token, self.player === player else { AudioSessionControl.release(token); return false }
                player.play(); startTimer()
            }
            isPlaying = player.isPlaying
            return player.isPlaying
        }
        stopPlayback()
        let token = UUID(); audioLease = token
        let prepared = try await Task.detached(priority: .userInitiated) { try PreparedAudio(url: url) }.value
        guard audioLease == token else { return false }
        try await AudioSessionControl.acquire(token, recording: false)
        guard audioLease == token, !hasRecording else { AudioSessionControl.release(token); return false }
        let player = prepared.player
        player.delegate = self
        guard player.play() else { throw CocoaError(.fileReadUnknown) }
        self.player = player; playingID = id; duration = player.duration; position = 0; isPlaying = true
        startTimer()
        return true
    }

    func seek(_ seconds: Double) { player?.currentTime = max(0, min(duration, seconds)); position = player?.currentTime ?? 0 }

    func stopPlayback() {
        player?.stop(); player = nil; playingID = nil; position = 0; duration = 0; isPlaying = false
        if !isRecording { timer?.invalidate(); timer = nil; AudioSessionControl.release(audioLease); audioLease = nil }
    }

    func startRecording(chat: String, quote: String?, shouldStart: () -> Bool) async {
        guard !hasRecording else { error = "Finish or discard the voice recording in the other chat first."; return }
        guard !requestingPermission else { return }
        requestingPermission = true
        defer { requestingPermission = false }
        guard await AVAudioApplication.requestRecordPermission() else {
            error = "Allow microphone access in iPhone Settings to record voice messages."; return
        }
        guard shouldStart(), !hasRecording else { return }
        do {
            stopPlayback()
            let token = UUID(); audioLease = token
            try await AudioSessionControl.acquire(token, recording: true)
            guard shouldStart(), audioLease == token else { AudioSessionControl.release(token); return }
            let url = try AttachmentImport.directory().appendingPathComponent(UUID().uuidString + ".wav")
            let recorder = try AVAudioRecorder(url: url, settings: [
                AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 48_000, AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16, AVLinearPCMIsFloatKey: false, AVLinearPCMIsBigEndianKey: false
            ])
            recorder.isMeteringEnabled = true
            guard recorder.record(forDuration: 600) else { throw CocoaError(.fileWriteUnknown) }
            self.recorder = recorder; recordingURL = url; pcmURL = nil
            recordingChat = chat; quotedMessageID = quote
            recordingID = UUID()
            recordingDuration = 0; levels = []; isRecording = true; hasRecording = true; startTimer()
        } catch { self.error = "Could not start the microphone."; discardRecording() }
    }

    private func startTimer() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                if let recorder = self.recorder, self.isRecording {
                    if recorder.currentTime > self.recordingDuration { self.recordingDuration = recorder.currentTime }
                    recorder.updateMeters()
                    let level = pow(10, Double(recorder.averagePower(forChannel: 0)) / 20)
                    self.levels = Array((self.levels + [level]).suffix(64))
                    if !recorder.isRecording { self.pauseForBackground() }
                } else if let player = self.player {
                    if self.position != player.currentTime { self.position = player.currentTime }
                    // An interruption can pause the player without a delegate callback.
                    if self.isPlaying != player.isPlaying { self.isPlaying = player.isPlaying }
                }
            }
        }
    }

    func pauseForBackground() {
        if isRecording {
            recordingDuration = max(recordingDuration, recorder?.currentTime ?? 0)
            recorder?.stop(); isRecording = false
        }
        player?.pause(); timer?.invalidate(); timer = nil
        if isPlaying { isPlaying = false }
        AudioSessionControl.release(audioLease); audioLease = nil
    }

    func finishRecording() async throws -> URL? {
        guard let recordingURL else { return nil }
        pauseForBackground()
        if let pcmURL { return pcmURL }
        let output = try await Task.detached(priority: .userInitiated) {
            let audio = try AVAudioFile(forReading: recordingURL, commonFormat: .pcmFormatFloat32, interleaved: false)
            guard audio.processingFormat.sampleRate == 48_000, audio.processingFormat.channelCount == 1,
                  audio.length > 0, audio.length <= 48_000 * 600 else { throw CocoaError(.fileReadCorruptFile) }
            let output = recordingURL.deletingPathExtension().appendingPathExtension("f32")
            FileManager.default.createFile(atPath: output.path, contents: nil, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
            let file = try FileHandle(forWritingTo: output)
            defer { try? file.close() }
            guard let buffer = AVAudioPCMBuffer(pcmFormat: audio.processingFormat, frameCapacity: 16_384) else { throw CocoaError(.coderInvalidValue) }
            while audio.framePosition < audio.length {
                try audio.read(into: buffer)
                guard buffer.frameLength > 0, let samples = buffer.floatChannelData?.pointee else { break }
                try file.write(contentsOf: Data(bytes: samples, count: Int(buffer.frameLength) * 4))
            }
            return output
        }.value
        guard self.recordingURL == recordingURL else { try? FileManager.default.removeItem(at: output); return nil }
        pcmURL = output
        return output
    }

    func releaseRecording(keeping path: URL) {
        if pcmURL == path { pcmURL = nil }
        if recordingURL == path { recordingURL = nil }
        discardRecording()
    }

    func discardRecording() {
        recorder?.stop(); recorder = nil; isRecording = false; hasRecording = false
        timer?.invalidate(); timer = nil
        if let recordingURL { try? FileManager.default.removeItem(at: recordingURL) }
        if let pcmURL { try? FileManager.default.removeItem(at: pcmURL) }
        recordingURL = nil; pcmURL = nil; recordingDuration = 0; levels = []
        recordingChat = nil; quotedMessageID = nil
        recordingID = nil
        AudioSessionControl.release(audioLease); audioLease = nil
    }

    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor in if self.player === player { self.stopPlayback() } }
    }
}

struct VoicePlaybackView: View {
    let message: Message
    @Environment(ChatStore.self) private var store
    let audio: NativeAudio
    var body: some View {
        VStack(spacing: 6) {
            HStack {
                Button { store.play(message) } label: {
                    Image(systemName: audio.playingID == message.id && audio.isPlaying ? "pause.fill" : "play.fill").font(.title2).frame(width: 36, height: 36)
                }.accessibilityLabel("Play or pause voice message")
                if audio.playingID == message.id {
                    Slider(value: Binding(get: { audio.position }, set: audio.seek), in: 0...max(1, audio.duration))
                } else {
                    HStack(alignment: .center, spacing: 2) {
                        ForEach(Array((message.content?.waveform ?? Array(repeating: 32, count: 48)).enumerated()), id: \.offset) { _, height in
                            Capsule().fill(Color.accentColor.opacity(0.7)).frame(width: 2, height: max(3, min(30, Double(height) / 100 * 30)))
                        }
                    }.frame(maxWidth: .infinity)
                }
            }
            Text(Duration.seconds(audio.playingID == message.id ? audio.position : message.content?.seconds ?? 0).formatted(.time(pattern: .minuteSecond)))
                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }.frame(maxWidth: 240)
    }
}
