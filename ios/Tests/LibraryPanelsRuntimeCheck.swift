#if DEBUG
import SwiftUI
import UIKit

/// Actual production SwiftUI panels in native sheet containers. Seeded confirmation
/// state is render evidence, explicitly not a successful Files/share interaction.
enum LibraryPanelsRuntimeCheck {
    @MainActor static func run() {
        Task { @MainActor in
            let files=FileManager.default
            let caches=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            do {
                for _ in 0..<100 {
                    if UIApplication.shared.connectedScenes.contains(where:{$0 is UIWindowScene}) {break}
                    try await Task.sleep(for:.milliseconds(20))
                }
                guard let scene=UIApplication.shared.connectedScenes.first(where:{$0 is UIWindowScene}) as? UIWindowScene else {throw CoreFailure(message:"No diagnostic window scene")}
                let source=files.urls(for:.documentDirectory,in:.userDomainMask)[0].appendingPathComponent("reading-modes.epub")
                try await capture(scene:scene,caches:caches,source:source)
                try JSONSerialization.data(withJSONObject:["passed":true,"production_panels_rendered":true,"file_picker_interaction_verified":false],options:[.prettyPrinted,.sortedKeys]).write(to:caches.appendingPathComponent("support-panels-verification.json"),options:.atomic)
            } catch {
                let report:[String:Any]=["passed":false,"error":String(describing:error)]
                try? JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:caches.appendingPathComponent("support-panels-verification.json"),options:.atomic)
            }
        }
    }
    @MainActor static func capture(scene:UIWindowScene,caches:URL,source:URL) async throws {
        guard ["local.reader.columns","local.reader.ui-files"].contains(Bundle.main.bundleIdentifier ?? "") else {throw CoreFailure(message:"Panel checks require the isolated diagnostic app")}
        let previousLanguage=ReaderI18n.shared.preference
        ReaderI18n.shared.setLanguage("zh_cn")
        defer {ReaderI18n.shared.setLanguage(previousLanguage)}
        let files=FileManager.default
        for (detail,key) in [("Unsupported library backup version","backup.error_version"),("Backup changed after verification; select and verify it again","backup.error_changed"),("No space left on device (os error 28)","backup.error_space"),("Permission denied (os error 13)","backup.error_permission"),("Invalid Zip archive","backup.error_hint")] {
            guard LibraryBackupView.failureHint(detail)==key else {throw CoreFailure(message:"Incorrect user-facing backup failure cause")}
        }
        let root=caches.appendingPathComponent("SupportPanels-"+UUID().uuidString)
        let backup=caches.appendingPathComponent(UUID().uuidString+".readerbackup")
        defer {try? files.removeItem(at:root);try? files.removeItem(at:backup)}
        let library=PreviewLibrary(rootOverride:root)
        let entry:LibraryEntry=try library.client.call("import",["path":source.path])
        let book:PreviewBook=try library.client.call("open",["id":entry.id])
        let blocks=try book.chapter(at:0).blocks
        guard let paragraph=blocks.enumerated().first(where:{$0.element.type=="Paragraph"&&($0.element.spans ?? []).map(\.text).joined().unicodeScalars.count>=4}) else {throw CoreFailure(message:"Panel fixture has no text")}
        let block=paragraph.element.core_block ?? paragraph.offset
        let saved:ReaderAnnotations=try library.client.call("annotations",["action":"highlight","id":entry.id,"chapter":0,"start_block":block,"end_block":block,"start_offset":0,"end_offset":4,"note":"Original note"])
        guard let highlight=saved.highlights.first else {throw CoreFailure(message:"Panel fixture highlight missing")}
        let _:ReaderAnnotations=try library.client.call("annotation_tools",["action":"note","id":entry.id,"highlight_id":highlight.id,"content":"Other device edit","original_note":"Original note"])
        let concurrent:ReaderAnnotations=try library.client.call("annotation_tools",["action":"note","id":entry.id,"highlight_id":highlight.id,"content":"Local draft","original_note":"Original note"])
        let annotations=ReadingAnnotations(library:library,book:book);annotations.value=concurrent
        await library.reload()
        let account=GitHubAccount(library:library)
        let appearance=AppearanceCoordinator()
        try await snapshot(AnnotationsView(annotations:annotations,current:.init(chapter:0,block:block),jump:{_ in}),scene:scene,caches:caches,name:"ios-annotations-panel.png",detent:.large,appearance:appearance)
        try await snapshot(AnnotationDetailsView(annotations:annotations,key:"n:"+highlight.id,highlightID:highlight.id),scene:scene,caches:caches,name:"ios-annotation-details.png",detent:.large,appearance:appearance)
        try await snapshot(LibraryBackupView(library:library,account:account),scene:scene,caches:caches,name:"ios-backup-panel.png",detent:.medium,appearance:appearance)
        let _:LibraryBackupSummary=try await library.store.backup(action:"export",path:backup.path)
        let verified=try await library.store.backup(action:"inspect",path:backup.path)
        try await snapshot(LibraryBackupView.verification(library:library,account:account,file:backup,summary:verified),scene:scene,caches:caches,name:"ios-backup-verified.png",detent:.large,appearance:appearance)
        try await snapshot(LibraryBackupView.verification(library:library,account:account,file:backup,summary:verified),scene:scene,caches:caches,name:"ios-backup-verified-medium.png",detent:.medium,appearance:appearance)
        try await snapshot(LibraryBackupView.verification(library:library,account:account,file:backup,summary:verified),scene:scene,caches:caches,name:"ios-backup-verified-large-type.png",detent:.large,appearance:appearance,type:.accessibility3)
    }
    @MainActor private static func snapshot<Content:View>(_ content:Content,scene:UIWindowScene,caches:URL,name:String,detent:PresentationDetent,appearance:AppearanceCoordinator,type:DynamicTypeSize = .large) async throws {
        let window=UIWindow(windowScene:scene)
        window.overrideUserInterfaceStyle = .light
        let host=UIHostingController(rootView:Panel(content:content,detent:detent,appearance:appearance,type:type))
        window.rootViewController=host;window.makeKeyAndVisible()
        defer {host.dismiss(animated:false);window.isHidden=true;window.rootViewController=nil}
        host.view.setNeedsLayout();host.view.layoutIfNeeded()
        for _ in 0..<150 {
            if let shown=host.presentedViewController,shown.view.window != nil,!shown.isBeingPresented {break}
            try await Task.sleep(for:.milliseconds(20))
        }
        guard host.presentedViewController?.view.window != nil else {throw CoreFailure(message:"Native panel was not presented")}
        // Let on-demand detail reads and SwiftUI invalidation settle after UIKit's
        // sheet transition. These waits are not performance measurements.
        try await Task.sleep(for:.milliseconds(300))
        let image=UIGraphicsImageRenderer(size:window.bounds.size).image {_ in window.drawHierarchy(in:window.bounds,afterScreenUpdates:true)}
        guard let bytes=image.pngData(),bytes.count>1024 else {throw CoreFailure(message:"Native panel screenshot missing")}
        try bytes.write(to:caches.appendingPathComponent(name),options:.atomic)
    }
    private struct Panel<Content:View>:View {
        let content:Content;let detent:PresentationDetent;let appearance:AppearanceCoordinator;let type:DynamicTypeSize
        var body:some View {
            Color(uiColor:.systemGroupedBackground).ignoresSafeArea()
                .sheet(isPresented:.constant(true)) {
                    NavigationStack {
                        content.toolbar {ToolbarItem(placement:.confirmationAction) {Button(L("settings.close")) {}}}
                    }.readerPanelSurface(appearance:appearance)
                        .environment(\.colorScheme,.light)
                        .environment(\.locale,ReaderI18n.shared.locale)
                        .environment(\.dynamicTypeSize,type)
                        .presentationDetents([detent]).presentationDragIndicator(.visible)
                }
        }
    }
}
#endif

#if DEBUG
/// Public UI tests use a separate bundle and owned cache root. No account or
/// production library is touched; the backup/share/import views remain real.
struct LibraryFilesUITestHost: View {
    @StateObject private var library: PreviewLibrary
    @StateObject private var account: GitHubAccount
    @State private var ready = false
    private let runID:String
    private let startsEmpty:Bool
    init() {
        precondition(Bundle.main.bundleIdentifier == "local.reader.ui-files")
        let args = CommandLine.arguments
        let index = args.firstIndex(of: "--ui-backup-run")!
        let run = args[index + 1]
        precondition(UUID(uuidString: run) != nil)
        runID=run;startsEmpty=args.contains("--ui-backup-empty")
        let files = FileManager.default
        let root = files.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("UIBackupLibrary-" + run, isDirectory: true)
        if args.contains("--ui-backup-empty") { try! files.removeItem(at: root) }
        let library = PreviewLibrary(rootOverride: root)
        if !args.contains("--ui-backup-empty") {
            let source = files.urls(for: .documentDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("reading-modes.epub")
            let _: LibraryEntry = try! library.client.call("import", ["path": source.path])
        }
        _library = StateObject(wrappedValue: library)
        _account = StateObject(wrappedValue: GitHubAccount(library: library))
    }
    var body: some View {
        NavigationStack {
            if ready { LibraryBackupView(library: library, account: account) }
            else { ProgressView().accessibilityIdentifier("backup-host-loading") }
        }.tint(.readerAppTint).task {
            await library.reload()
            let caches=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            let catalog=caches.appendingPathComponent("UIBackupLibrary-"+runID).appendingPathComponent("library.json")
            let state:[String:Any]=["run_id":runID,"empty_start":startsEmpty,"initial_books":library.books.count,
                                    "initial_catalog_base64":(try? Data(contentsOf:catalog).base64EncodedString()) ?? ""]
            let name=startsEmpty ? "ui-backup-empty-run.json" : "ui-backup-seed-"+runID+".json"
            try! JSONSerialization.data(withJSONObject:state).write(to:caches.appendingPathComponent(name),options:.atomic)
            ready = true
        }
    }
}
#endif
