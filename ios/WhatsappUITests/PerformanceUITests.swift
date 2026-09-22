import XCTest

final class PerformanceUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        #if !BENCHMARK
        throw XCTSkip("Run the optimized, offline Benchmark configuration explicitly")
        #endif
    }

    private var options: XCTMeasureOptions {
        let options = XCTMeasureOptions(); options.iterationCount = 3; return options
    }

    private func fixture() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--demo", "--performance-fixture"]
        return app
    }

    func testOfflineLaunch() {
        let app = fixture()
        measure(metrics: [XCTApplicationLaunchMetric(waitUntilResponsive: true)], options: options) {
            app.launch()
        }
        XCTAssertTrue(app.buttons["chat-performance-0@g.us"].exists)
    }

    func testLongConversationScrolling() {
        let app = fixture(); app.launch()
        app.buttons["chat-performance-0@g.us"].tap()
        let scroll = app.scrollViews["conversation-scroll"]
        XCTAssertTrue(scroll.waitForExistence(timeout: 10))
        measure(metrics: [XCTOSSignpostMetric.scrollingAndDecelerationMetric, XCTCPUMetric(application: app), XCTMemoryMetric(application: app)], options: options) {
            for _ in 0..<4 { scroll.swipeDown(velocity: .fast) }
            for _ in 0..<4 { scroll.swipeUp(velocity: .fast) }
        }
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Long conversation after measured scrolling — offline fixture"
        screenshot.lifetime = .keepAlways; add(screenshot)
    }
}
