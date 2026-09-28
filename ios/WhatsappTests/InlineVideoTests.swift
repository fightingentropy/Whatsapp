import AVFoundation
import XCTest
@testable import Whatsapp

@MainActor final class InlineVideoTests: XCTestCase {
    func testNativePlaybackDecodesVideoAndAudioAndReleasesOnClose() async throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "inline-video", withExtension: "mp4"))
        let video = NativeVideo()
        defer { video.stop() }
        await video.play(url, id: "first")
        let player = try XCTUnwrap(video.player)
        player.isMuted = true
        let item = try XCTUnwrap(player.currentItem)
        let output = AVPlayerItemVideoOutput(pixelBufferAttributes: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        item.add(output)
        var frame: CVPixelBuffer?
        let deadline = Date().addingTimeInterval(8)
        while Date() < deadline && (frame == nil || player.currentTime().seconds < 0.15) {
            try await Task.sleep(for: .milliseconds(40))
            if let decoded = output.copyPixelBuffer(forItemTime: player.currentTime(), itemTimeForDisplay: nil) { frame = decoded }
        }
        let pixels = try XCTUnwrap(frame, "Playback must produce decoded pixels")
        XCTAssertEqual(CVPixelBufferGetWidth(pixels), 320)
        XCTAssertEqual(CVPixelBufferGetHeight(pixels), 180)
        CVPixelBufferLockBaseAddress(pixels, .readOnly)
        let bytes = try XCTUnwrap(CVPixelBufferGetBaseAddress(pixels)).assumingMemoryBound(to: UInt8.self)
        XCTAssertTrue(stride(from: 0, to: CVPixelBufferGetBytesPerRow(pixels) * 180, by: 4).contains { Int(bytes[$0]) + Int(bytes[$0 + 1]) + Int(bytes[$0 + 2]) > 100 })
        CVPixelBufferUnlockBaseAddress(pixels, .readOnly)
        XCTAssertGreaterThan(player.currentTime().seconds, 0)

        let asset = AVURLAsset(url: url)
        let tracks = try await asset.loadTracks(withMediaType: .audio)
        let track = try XCTUnwrap(tracks.first)
        let reader = try AVAssetReader(asset: asset)
        let audio = AVAssetReaderTrackOutput(track: track, outputSettings: [AVFormatIDKey: kAudioFormatLinearPCM])
        reader.add(audio); XCTAssertTrue(reader.startReading())
        let sample = try XCTUnwrap(audio.copyNextSampleBuffer())
        XCTAssertGreaterThan(CMSampleBufferGetNumSamples(sample), 0, "The fixture must decode an audio track too")
        reader.cancelReading()

        video.pause("first"); XCTAssertEqual(player.rate, 0)
        await video.play(url, id: "second")
        XCTAssertNil(player.currentItem, "Replacing a video releases the previous decoder")
        XCTAssertEqual(video.id, "second")
        video.stop(); XCTAssertNil(video.player); XCTAssertNil(video.id)
    }

    func testChatCloseAndBackgroundReleaseVideo() async throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "inline-video", withExtension: "mp4"))
        let store = ChatStore(demo: true)
        store.open("weekend@g.us")
        await store.video.play(url, id: "fixture")
        store.close("weekend@g.us")
        XCTAssertNil(store.video.player)
        await store.video.play(url, id: "fixture")
        store.background()
        XCTAssertNil(store.video.player)
    }
}
