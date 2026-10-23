import XCTest

final class TxtImportUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: ProcessInfo.processInfo.environment["READER_UI_TARGET_BUNDLE"] ?? "local.reader.txt")
    override func setUpWithError() throws { continueAfterFailure = false }
    private func launch() {
        app.launchArguments = ["--verify-txt-import-ui", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["txt-convert"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.staticTexts["txt-count"].waitForExistence(timeout: 30))
    }
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testPreviewExclusionManualAdditionAndConversion() {
        launch()
        XCTAssertTrue(app.staticTexts["txt-count"].label.contains("4")); shot("txt-preview")
        let first = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "txt-toggle-")).firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 10)); first.tap()
        XCTAssertTrue(app.buttons["txt-convert"].waitForExistence(timeout: 10))
        // Search the original source and select a line independently of its format.
        app.buttons["txt-add-heading"].tap()
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 10)); search.tap(); search.typeText("§")
        let source = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "txt-source-")).firstMatch
        XCTAssertTrue(source.waitForExistence(timeout: 15)); shot("txt-source-search"); source.tap()
        let convert = app.buttons["txt-convert"]
        let enabled = NSPredicate(format: "enabled == true")
        expectation(for: enabled, evaluatedWith: convert)
        waitForExpectations(timeout: 20)
        convert.tap()
        XCTAssertTrue(app.staticTexts["txt-imported-count"].waitForExistence(timeout: 30))
        XCTAssertEqual(app.staticTexts["txt-imported-count"].label, "1")
        shot("txt-import-complete")
    }
    func testInvalidRuleIsVisibleAndCanBeCorrected() {
        launch()
        app.buttons["Advanced"].tap()
        let regex = app.textFields["txt-regex"]
        XCTAssertTrue(regex.waitForExistence(timeout: 10)); regex.tap(); regex.typeText("[")
        XCTAssertTrue(app.staticTexts["txt-error"].waitForExistence(timeout: 15))
        XCTAssertFalse(app.buttons["txt-convert"].isEnabled)
        shot("txt-invalid-rule")
        regex.tap(); regex.typeText(XCUIKeyboardKey.delete.rawValue)
        expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: app.buttons["txt-convert"])
        waitForExpectations(timeout: 15)
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.staticTexts["txt-imported-count"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.staticTexts["txt-imported-count"].label, "0")
    }
    func testNativeServiceSettingsAndLookupMissingConfiguration(){
        app.launchArguments=["--verify-parity-ui","-AppleLanguages","(en)","-AppleLocale","en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["parity-translate"].waitForExistence(timeout:20));app.buttons["parity-translate"].tap()
        XCTAssertTrue(app.staticTexts["Set a service URL in reading settings."].waitForExistence(timeout:10));shot("ios-native-translation-unconfigured")
        let settings=app.buttons["Dictionary & translation services"]
        XCTAssertTrue(settings.waitForExistence(timeout:10));settings.tap()
        XCTAssertTrue(app.secureTextFields.firstMatch.waitForExistence(timeout:10));XCTAssertEqual(app.secureTextFields.count,2)
        XCTAssertEqual(app.textFields.count,4);shot("ios-native-service-settings")
        let endpoint=app.textFields["service-translate-url"]
        XCTAssertTrue(endpoint.exists);endpoint.tap();endpoint.typeText("file:///test-configured-on-return")
        app.navigationBars.buttons.element(boundBy:0).tap()
        XCTAssertTrue(app.buttons["Retry"].waitForExistence(timeout:15))
        shot("ios-service-configuration-return")
    }

    func testNativeServiceRejectsUnsafeURLAndOffersRetry(){
        app.launchArguments=["--verify-parity-ui","-AppleLanguages","(en)","-AppleLocale","en_US"];app.launch()
        XCTAssertTrue(app.buttons["parity-invalid-service"].waitForExistence(timeout:20));app.buttons["parity-invalid-service"].tap()
        XCTAssertTrue(app.staticTexts["Query failed. Check the service settings or try again later."].waitForExistence(timeout:15))
        let retry=app.buttons["Retry"];XCTAssertTrue(retry.exists);retry.tap()
        XCTAssertTrue(app.staticTexts["Query failed. Check the service settings or try again later."].waitForExistence(timeout:15));shot("ios-native-service-retry")
    }
    func testContributionPreviewCanRemoveSampleBeforeSubmitting(){
        app.launchArguments=["--verify-parity-ui","-AppleLanguages","(en)","-AppleLocale","en_US"];app.launch()
        XCTAssertTrue(app.buttons["parity-samples"].waitForExistence(timeout:30));app.buttons["parity-samples"].tap()
        let sample=app.descendants(matching:.any).matching(identifier:"contribution-sample-0").firstMatch
        XCTAssertTrue(sample.waitForExistence(timeout:15));shot("ios-contribution-review")
        sample.swipeLeft();let delete=app.buttons["Delete"].firstMatch;XCTAssertTrue(delete.waitForExistence(timeout:5));delete.tap()
        XCTAssertTrue(app.staticTexts["No accepted corrections to contribute"].waitForExistence(timeout:10));XCTAssertFalse(app.buttons["Confirm & submit to GitHub"].isEnabled)
    }

}
