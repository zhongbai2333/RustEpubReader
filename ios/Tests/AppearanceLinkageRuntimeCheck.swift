#if DEBUG
import UIKit

enum AppearanceLinkageRuntimeCheck {
    @MainActor static func run(_ coordinator: AppearanceCoordinator, close: () -> Void) async {
        let out=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0].appendingPathComponent("linkage-checks")
        try? FileManager.default.createDirectory(at:out,withIntermediateDirectories:true)
        var report:[String:Any]=[:]
        func find(_ controller:UIViewController)->NativeReadingController? {
            if let native=controller as? NativeReadingController{return native}
            for child in controller.children {if let found=find(child){return found}}
            if let presented=controller.presentedViewController {return find(presented)}
            return nil
        }
        func window()->UIWindow? {UIApplication.shared.connectedScenes.compactMap{$0 as? UIWindowScene}.flatMap{$0.windows}.first{$0.isKeyWindow}}
        func native()->NativeReadingController? {window()?.rootViewController.flatMap{find($0)}}
        func wait(_ label:String,_ predicate:()->Bool) async throws {
            for _ in 0..<160 {if predicate(){return};try await Task.sleep(for:.milliseconds(50))}
            throw CoreFailure(message:label)
        }
        func capture(_ name:String) async throws {
            try await Task.sleep(for:.milliseconds(350))
            guard let window=window() else{return}
            window.layoutIfNeeded()
            let renderer=UIGraphicsImageRenderer(bounds:window.bounds)
            _=renderer.image{_ in window.drawHierarchy(in:window.bounds,afterScreenUpdates:true)}
            try await Task.sleep(for:.milliseconds(100))
            let image=renderer.image{_ in window.drawHierarchy(in:window.bounds,afterScreenUpdates:true)}
            try image.pngData()?.write(to:out.appendingPathComponent(name+".png"))
        }
        func paintMatches()->Bool {
            let style=ReadingAppearance.decode(UserDefaults.standard.string(forKey:ReadingAppearance.storageKey) ?? "").style[dark:coordinator.readingDark]
            return native()?.options.backgroundHex==style.paper
        }
        do {
            try await wait("Reader did not mount"){native() != nil}
            guard let initial=native() else{throw CoreFailure(message:"No reader")}
            let anchor=initial.anchor, font=initial.options.fontSize, system=coordinator.systemDark
            if CommandLine.arguments.contains("--expected-system-dark") && !system { throw CoreFailure(message:"System source does not match simulator dark appearance") }
            coordinator.value=AppearanceLinkage()
            try await wait("Initial reading paint did not follow source",paintMatches)
            try await capture("linked-system")
            coordinator.value.source = .ambient
            try await capture("ambient-unavailable")
            guard coordinator.value.source == .ambient,coordinator.readingDark==system else{throw CoreFailure(message:"Unavailable source preference")}
            coordinator.value.source = .system
            coordinator.toggleReading()
            try await wait("Linked shortcut did not update native paint",paintMatches)
            guard !coordinator.value.appLinked,coordinator.value.readingLinked,coordinator.appDark == !system,coordinator.readingDark == !system else{throw CoreFailure(message:"Linked shortcut scope")}
            coordinator.value=AppearanceLinkage(appLinked:false,readingLinked:false,appManual:true,readingMode:.light)
            try await wait("Independent reading paint did not apply",paintMatches)
            guard coordinator.appDark && !coordinator.readingDark else{throw CoreFailure(message:"Independent reading scope")}
            try await capture("independent-light")
            coordinator.value.readingMode = .automatic;coordinator.value.readingSource = .ambient
            try await wait("Unavailable ambient fallback did not apply",paintMatches)
            guard coordinator.readingDark==system,coordinator.value.readingSource == .ambient else{throw CoreFailure(message:"Fallback changed preference")}
            coordinator.toggleReadingLink()
            try await wait("Reading relink did not apply",paintMatches)
            coordinator.toggleReading()
            try await wait("Final shortcut paint",paintMatches)
            guard native()?.anchor==anchor,native()?.options.fontSize==font else{throw CoreFailure(message:"Anchor or typography changed")}
            guard AppearanceLinkage.decode(UserDefaults.standard.string(forKey:AppearanceLinkage.storageKey))==coordinator.value else{throw CoreFailure(message:"Linkage not persisted")}
            close()
            try await wait("Reader did not close"){native()==nil}
            try await Task.sleep(for:.milliseconds(1200))
            guard !coordinator.appDark,coordinator.systemDark==system else{throw CoreFailure(message:"Exit changed app or system appearance")}
            try await capture("library-after-shortcut")
            report=["passed":true,"production_host":true,"native_paint_updated":true,"linked_shortcut_and_exit":true,"independent_reading":true,"ambient_fallback_preserves_preference":true,"relink":true,"persisted":true,"anchor_and_font_preserved":true]
        } catch {report=["passed":false,"error":String(describing:error)]}
        try? JSONSerialization.data(withJSONObject:report,options:.prettyPrinted).write(to:out.appendingPathComponent("result.json"))
    }
}
#endif
