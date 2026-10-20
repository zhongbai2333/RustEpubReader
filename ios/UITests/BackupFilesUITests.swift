import XCTest

/// Tests require the isolated local.reader.ui-files app on a disposable simulator.
/// The runner provisions an owned EPUB and malformed backup in its Documents.
final class BackupFilesUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: "local.reader.ui-files")
    private let runID = UUID().uuidString
    override func setUpWithError() throws { continueAfterFailure = false }
    private func launch(empty: Bool = false) {
        app.launchArguments = ["--verify-backup-files-ui", "--book-share-preview", "--ui-backup-run", runID,
                               "-reader.language", "en", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        if empty { app.launchArguments.append("--ui-backup-empty") }
        app.launch()
        XCTAssertTrue(app.buttons["backup-create"].waitForExistence(timeout: 30))
    }
    private func screenshot(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    private func tapVisible(_ element: XCUIElement) {
        for _ in 0..<6 {
            if element.exists && element.isHittable { element.tap(); return }
            app.swipeUp()
        }
        XCTFail("Control not reachable: \(element)")
    }
    private func openFileCell(_ cell:XCUIElement) {
        // In the current system icon layout, the cell's centre falls in the
        // caption area. Target the visible icon, not unused caption space.
        if cell.frame.height > 100 { cell.coordinate(withNormalizedOffset: CGVector(dx:0.5,dy:0.25)).tap() }
        else { cell.tap() }
    }
    private func openDocuments() {
        // The public system picker loads locations asynchronously. Missing
        // locations must fail the test, rather than silently skip navigation.
        let onPad = app.cells["DOC.sidebar.item.On My iPad"]
        XCTAssertTrue(onPad.waitForExistence(timeout: 30)); onPad.tap()
        let directory = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@", "Reader Test Files,")).firstMatch
        XCTAssertTrue(directory.waitForExistence(timeout: 30)); openFileCell(directory)
        let opened=app.otherElements.matching(NSPredicate(format:"identifier == %@","DOC.browsingRoot Source: com.apple.FileProvider.LocalStorage, Title: Reader Test Files")).firstMatch
        print("APP FILES AFTER FOLDER ACTION\n"+app.debugDescription)
        XCTAssertTrue(opened.waitForExistence(timeout:20),"System picker did not enter the external fixture folder")
    }
    func testSystemFilesCanBrowseOwnedFolder() throws {
        let files=XCUIApplication(bundleIdentifier:"com.apple.DocumentsApp")
        files.launchArguments=["-AppleLanguages","(en)","-AppleLocale","en_US"]
        files.launch()
        let browse=files.buttons["Browse"].firstMatch
        if browse.waitForExistence(timeout:5) {browse.tap()}
        print("SYSTEM FILES BEFORE NAVIGATION\n"+files.debugDescription)
        let local=files.cells.matching(NSPredicate(format:"label CONTAINS %@","iPad")).firstMatch
        if !local.waitForExistence(timeout:5) {
            let sidebar=files.buttons.matching(NSPredicate(format:"label CONTAINS[c] %@","sidebar")).firstMatch
            XCTAssertTrue(sidebar.waitForExistence(timeout:5));sidebar.tap()
        }
        XCTAssertTrue(local.waitForExistence(timeout:15));local.tap()
        let folder=files.cells.matching(NSPredicate(format:"label BEGINSWITH %@","Reader Test Files,")).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout:15));openFileCell(folder)
        let opened=files.otherElements.matching(NSPredicate(format:"identifier CONTAINS %@","Title: Reader Test Files")).firstMatch
        print("SYSTEM FILES AFTER FOLDER ACTION\n"+files.debugDescription)
        let image=XCTAttachment(screenshot:files.screenshot());image.name="system-files-control";image.lifetime = .keepAlways;add(image)
        XCTAssertTrue(opened.waitForExistence(timeout:20),"The system Files app did not open the owned local folder")
    }
    func testBackupPanelAccessibilityAudit() throws {
        launch()
        try app.performAccessibilityAudit(for: [.hitRegion,.dynamicType,.textClipped,.sufficientElementDescription,.contrast]) {issue in
            print("ACCESSIBILITY ISSUE: " + issue.compactDescription + " / " + issue.detailedDescription)
            if let element=issue.element {print(element.debugDescription)}
            return false
        }
        screenshot("backup-panel-accessibility-audit")
    }
    func testExportSaveToFilesInspectAndMerge() throws {
        launch()
        app.buttons["backup-create"].tap()
        let saveToFiles = app.cells.matching(NSPredicate(format: "label IN %@", ["Save to Files", "保存到“文件”"])).firstMatch
        XCTAssertTrue(saveToFiles.waitForExistence(timeout: 30)); openFileCell(saveToFiles)
        screenshot("system-save-to-files")
        openDocuments()
        let save = app.buttons.matching(NSPredicate(format: "label IN %@", ["Save", "保存", "存储"])).firstMatch
        XCTAssertTrue(save.waitForExistence(timeout: 10)); save.tap()
        XCTAssertTrue(save.waitForNonExistence(timeout:20))
        let backToTasks=app.buttons["backup-home"]
        XCTAssertTrue(backToTasks.waitForExistence(timeout:10));backToTasks.tap()
        XCTAssertTrue(app.buttons["backup-choose"].waitForExistence(timeout:10))
        app.terminate(); launch(empty: true)
        app.buttons["backup-choose"].tap(); openDocuments()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@", "RustEpubReader")).firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 15)); openFileCell(file)
        XCTAssertTrue(app.staticTexts["backup-verified"].waitForExistence(timeout: 20))
        screenshot("real-package-verified")
        tapVisible(app.buttons["backup-merge"])
        let result = app.staticTexts["backup-result"]
        XCTAssertTrue(result.waitForExistence(timeout: 20)); XCTAssertEqual(result.label,"Merged into library"); screenshot("real-package-merged")
        XCTAssertFalse(app.staticTexts["backup-failure"].exists)
    }
    func testMalformedPackageDoesNotOfferMerge() throws {
        launch()
        app.buttons["backup-choose"].tap(); openDocuments()
        print("FILES PICKER HIERARCHY\n" + app.debugDescription)
        screenshot("malformed-file-picker")
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@", "malformed")).firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 15)); openFileCell(file)
        XCTAssertTrue(app.staticTexts["backup-failure"].waitForExistence(timeout: 20))
        XCTAssertFalse(app.buttons["backup-merge"].exists)
        screenshot("malformed-package-rejected")
    }
}
