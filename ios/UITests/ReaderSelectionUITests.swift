import XCTest

final class ReaderSelectionUITests:XCTestCase {
    private var readerBundle:String {ProcessInfo.processInfo.environment["READER_UI_TARGET_BUNDLE"] ?? "local.reader.columns"}
    private func readerApp() throws -> XCUIApplication {
        guard ["local.reader.columns","local.reader.ui-files"].contains(readerBundle) else {throw NSError(domain:"ReaderUITests",code:1)}
        let app=XCUIApplication(bundleIdentifier:readerBundle)
        for key in ["READER_SELECTION_EPUB_BASE64","READER_NAVIGATION_EPUB_BASE64"] {
            app.launchEnvironment[key]=try XCTUnwrap(ProcessInfo.processInfo.environment[key],"Missing admitted fixture")
        }
        return app
    }
    override func setUpWithError() throws {continueAfterFailure=false}
    override func tearDownWithError() throws {
        if (testRun?.failureCount ?? 0)>0 {
            for app in [XCUIApplication(bundleIdentifier:readerBundle),XCUIApplication(bundleIdentifier:"com.apple.Preferences")] where app.state == .runningForeground {
                let image=XCTAttachment(screenshot:app.screenshot());image.name="failed-input-screen";image.lifetime = .keepAlways;add(image)
                let hierarchy=XCTAttachment(string:app.debugDescription);hierarchy.name="failed-input-hierarchy";hierarchy.lifetime = .keepAlways;add(hierarchy)
            }
        }
    }
    private func waitForTouchTarget(_ element:XCUIElement,timeout:TimeInterval=10) {
        expectation(for:NSPredicate {_,_ in element.exists && element.isHittable},evaluatedWith:element)
        waitForExpectations(timeout:timeout)
    }
    private func launchAfterExit(_ app:XCUIApplication) {
        if app.state != .notRunning {app.terminate()}
        XCTAssertTrue(app.wait(for:.notRunning,timeout:20),"Previous process must exit before another launch")
        XCUIDevice.shared.press(.home)
        let home=XCUIApplication(bundleIdentifier:"com.apple.springboard")
        XCTAssertTrue(home.wait(for:.runningForeground,timeout:20),"Wait for the system workspace before launching")
        app.launch()
        app.activate()
        XCTAssertTrue(app.wait(for:.runningForeground,timeout:20),"Activate the launched app before querying or sending input")
    }
    private func select(_ mode:String) throws {
        let app=try readerApp()
        app.launchArguments=["--verify-selection-ui","--selection-mode",mode,"--selection-run",UUID().uuidString,"--book-share-preview","-reader.language","en"]
        launchAfterExit(app)
        let status=app.descendants(matching:.any).matching(identifier:"selection-status").firstMatch
        XCTAssertTrue(status.waitForExistence(timeout:30))
        let paper=app.descendants(matching:.any).matching(identifier:"native-selection-paper").firstMatch
        XCTAssertTrue(paper.waitForExistence(timeout:20),app.debugDescription)
        expectation(for:NSPredicate(format:"label == %@","Ready"),evaluatedWith:status)
        waitForExpectations(timeout:10)
        // A visible paper does not prove that the simulator has delivered
        // input to this app's window. Require one acknowledged native tap
        // before admitting the long-press/drag, without retrying either input.
        status.tap()
        expectation(for:NSPredicate {element,_ in
            guard let status=element as? XCUIElement,let raw=status.value as? String,
                  let data=raw.data(using:.utf8),let value=try? JSONSerialization.jsonObject(with:data) as? [String:Any] else{return false}
            return value["input_focus_confirmed"] as? Bool == true && value["key_window"] as? Bool == true && value["foreground_scene"] as? Bool == true
        },evaluatedWith:status)
        waitForExpectations(timeout:10)
        waitForTouchTarget(paper)
        let raw=try XCTUnwrap(status.value as? String)
        let points=try XCTUnwrap(JSONSerialization.jsonObject(with:Data(raw.utf8)) as? [String:Any])
        let first=try XCTUnwrap(points["start"] as? [Double]),last=try XCTUnwrap(points["end"] as? [Double])
        XCTAssertEqual(first.count,2);XCTAssertEqual(last.count,2)
        let start=paper.coordinate(withNormalizedOffset:CGVector(dx:first[0],dy:first[1]))
        let end=paper.coordinate(withNormalizedOffset:CGVector(dx:last[0],dy:last[1]))
        // Use a deliberate hold and slow drag, including a held endpoint. A
        // fast drag immediately followed by touch-up can be coalesced on a
        // loaded Simulator before UIKit delivers the selection's changed state.
        start.press(forDuration:1.2,thenDragTo:end,withVelocity:.slow,thenHoldForDuration:0.2)
        let passed=NSPredicate(format:"label == %@","Passed: 文字 😀 中文")
        expectation(for:passed,evaluatedWith:status)
        waitForExpectations(timeout:15)
        let result=try XCTUnwrap(status.value as? String)
        let value=try XCTUnwrap(JSONSerialization.jsonObject(with:Data(result.utf8)) as? [String:Any])
        XCTAssertEqual(value["passed"] as? Bool,true);XCTAssertEqual(value["persisted"] as? Bool,true)
        XCTAssertEqual(value["text"] as? String,"文字 😀 中文")
        let image=XCTAttachment(screenshot:app.screenshot());image.name="native-selection-"+mode;image.lifetime = .keepAlways;add(image)
        app.terminate()
        XCTAssertTrue(app.wait(for:.notRunning,timeout:20))
    }
    func testNoAnimationSelection() throws {try select("none")}
    func testCurlSelection() throws {try select("curl")}
    func testSlideSelection() throws {try select("slide")}
    func testCoverSelection() throws {try select("cover")}
    func testScrollSelection() throws {try select("scroll")}
    func testSystemReducedMotionNavigation() throws {
        let settings=XCUIApplication(bundleIdentifier:"com.apple.Preferences")
        settings.launchArguments=["-AppleLanguages","(en)","-AppleLocale","en_US"]
        func openMotionSwitch()->XCUIElement {
            launchAfterExit(settings)
            let toggle=settings.switches["Reduce Motion"].firstMatch
            // Settings can be reclaimed while the reader is foreground. A new
            // process may return to its root instead of the previous subpage.
            if !toggle.waitForExistence(timeout:2) {
                let accessibility=settings.cells.staticTexts["Accessibility"].firstMatch
                if !accessibility.waitForExistence(timeout:5) {settings.swipeUp()}
                XCTAssertTrue(accessibility.waitForExistence(timeout:10),settings.debugDescription);accessibility.tap()
                let motion=settings.cells.staticTexts["Motion"].firstMatch
                XCTAssertTrue(motion.waitForExistence(timeout:10),settings.debugDescription);motion.tap()
            }
            XCTAssertTrue(toggle.waitForExistence(timeout:10),settings.debugDescription)
            return toggle
        }
        let toggle=openMotionSwitch()
        // Settings exposes both a row-sized switch and its actual UISwitch.
        // Touch the real control, not a guessed percentage of the row frame.
        // A short held touch remains a tap while surviving host scheduling gaps.
        func tapSwitch(_ toggle:XCUIElement) {
            let knob=toggle.switches.firstMatch
            let target=knob.exists ? knob:toggle
            waitForTouchTarget(target)
            target.press(forDuration:0.2)
        }
        let hierarchy=XCTAttachment(string:toggle.debugDescription);hierarchy.name="system-motion-switch";hierarchy.lifetime = .keepAlways;add(hierarchy)
        let original=try XCTUnwrap(toggle.value as? String)
        XCTAssertTrue(["0","1"].contains(original))
        defer {
            let restored=openMotionSwitch()
            if restored.value as? String != original {tapSwitch(restored)}
            expectation(for:NSPredicate(format:"value == %@",original),evaluatedWith:restored)
            waitForExpectations(timeout:10)
            XCTAssertEqual(restored.value as? String,original,"Restore the owned simulator setting")
            settings.terminate()
        }
        if original=="0" {tapSwitch(toggle)}
        expectation(for:NSPredicate(format:"value == %@","1"),evaluatedWith:toggle)
        waitForExpectations(timeout:10)
        XCTAssertEqual(toggle.value as? String,"1")
        let setting=XCTAttachment(screenshot:settings.screenshot());setting.name="system-reduce-motion-enabled";setting.lifetime = .keepAlways;add(setting)
        let app=try readerApp()
        app.launchArguments=["--verify-reduced-motion-ui","--book-share-preview","-reader.language","en"]
        launchAfterExit(app)
        let status=app.staticTexts["motion-status"]
        XCTAssertTrue(status.waitForExistence(timeout:30))
        expectation(for:NSPredicate(format:"label == %@","Passed native reduced motion"),evaluatedWith:status)
        waitForExpectations(timeout:80)
        let raw=try XCTUnwrap(status.value as? String)
        let value=try XCTUnwrap(JSONSerialization.jsonObject(with:Data(raw.utf8)) as? [String:Any])
        XCTAssertEqual(value["passed"] as? Bool,true)
        XCTAssertEqual(value["system_reduce_motion"] as? Bool,true)
        XCTAssertEqual(value["reduced_motion_geometry_checks"] as? Int,15)
        let image=XCTAttachment(screenshot:app.screenshot());image.name="system-reduced-motion";image.lifetime = .keepAlways;add(image)
        app.terminate()
        // Exercise restoration after process loss on every iteration, instead
        // of relying on Settings retaining its navigation state in background.
        settings.terminate()
    }
}
