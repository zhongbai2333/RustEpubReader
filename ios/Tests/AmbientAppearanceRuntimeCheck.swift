#if DEBUG
import UIKit

enum AmbientAppearanceRuntimeCheck {
    final class Sensor: AmbientLightReading {
        // Start inside the hysteresis band to verify a real light decision at startup.
        var level: Double? = 7
        var starts = 0, stops = 0
        func start() -> Bool { starts += 1; return true }
        func read() -> Double? { level }
        func stop() { stops += 1 }
    }
    @MainActor static let sensor = Sensor()

    @MainActor private static func native() -> NativeReadingController? {
        func find(_ controller: UIViewController) -> NativeReadingController? {
            if let native = controller as? NativeReadingController { return native }
            for child in controller.children { if let found = find(child) { return found } }
            return controller.presentedViewController.flatMap(find)
        }
        return window()?.rootViewController.flatMap(find)
    }
    @MainActor private static func window() -> UIWindow? {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }.first { $0.isKeyWindow }
    }
    @MainActor private static func paintMatches(_ coordinator: AppearanceCoordinator) -> Bool {
        let style = ReadingAppearance.decode(UserDefaults.standard.string(forKey: ReadingAppearance.storageKey) ?? "").style
        return native()?.options.backgroundHex == style[dark: coordinator.readingDark].paper
    }
    @MainActor private static func wait(_ message: String, _ predicate: () -> Bool) async throws {
        for _ in 0..<180 {
            if predicate() { return }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw CoreFailure(message: message)
    }
    @MainActor private static func capture(_ file: URL) {
        guard let window = window() else { return }
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        try? image.pngData()?.write(to: file)
    }

    @MainActor static func run(_ coordinator: AppearanceCoordinator, physical: Bool) async {
        let out = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        var report: [String: Any] = ["physical": physical]
        func save() {
            try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
                .write(to: out.appendingPathComponent("ambient-reader-check.json"), options: .atomic)
        }
        do {
            try await wait("Reader did not mount") { native() != nil }
            let anchor = native()?.anchor, font = native()?.options.fontSize
            coordinator.value = AppearanceLinkage(source: .ambient)
            if physical {
                var samples: [[String: Any]] = []
                var captured: Set<Bool> = []
                for _ in 0..<180 {
                    let row: [String: Any] = ["uptime": ProcessInfo.processInfo.systemUptime,
                        "rawLevel": coordinator.ambientLevel.map { $0 as Any } ?? NSNull(),
                        "available": coordinator.sensorAvailable,
                        "ambientDark": coordinator.ambientDark.map { $0 as Any } ?? NSNull(),
                        "systemDark": coordinator.systemDark, "appDark": coordinator.appDark,
                        "readingDark": coordinator.readingDark, "nativePaintMatches": paintMatches(coordinator)]
                    samples.append(row)
                    report["samples"] = samples
                    report["sourceRetained"] = coordinator.value.source == .ambient
                    report["anchorPreserved"] = native()?.anchor == anchor
                    report["fontPreserved"] = native()?.options.fontSize == font
                    if let dark = coordinator.ambientDark, paintMatches(coordinator), captured.insert(dark).inserted {
                        capture(out.appendingPathComponent(dark ? "ambient-dark.png" : "ambient-light.png"))
                    }
                    report["bothAppearancesObserved"] = captured.count == 2
                    save()
                    try await Task.sleep(for: .seconds(1))
                }
                report["completed"] = true
            } else {
                try await wait("Bright sensor did not update native reader") {
                    coordinator.ambientDark == false && !coordinator.appDark && !coordinator.readingDark && paintMatches(coordinator)
                }
                capture(out.appendingPathComponent("ambient-light.png"))
                sensor.level = 10
                try await Task.sleep(for: .seconds(6))
                guard coordinator.ambientDark == false && !coordinator.readingDark && paintMatches(coordinator) else {
                    throw CoreFailure(message: "Dim indoor reading incorrectly selected dark")
                }
                sensor.level = 0
                try await Task.sleep(for: .seconds(2))
                sensor.level = 7
                try await Task.sleep(for: .seconds(4))
                guard coordinator.ambientDark == false && !coordinator.readingDark && paintMatches(coordinator) else {
                    throw CoreFailure(message: "Brief covering incorrectly selected dark")
                }
                sensor.level = 0
                try await wait("Dark sensor did not update native reader") {
                    coordinator.ambientDark == true && coordinator.appDark && coordinator.readingDark && paintMatches(coordinator)
                }
                capture(out.appendingPathComponent("ambient-dark.png"))
                coordinator.toggleAppLink()
                coordinator.value.appManual = false
                coordinator.toggleReadingLink()
                coordinator.value.readingMode = .automatic
                coordinator.value.readingSource = .ambient
                try await wait("Independent ambient reading did not apply") {
                    !coordinator.appDark && coordinator.readingDark && paintMatches(coordinator)
                }
                sensor.level = 13
                try await wait("Independent reading did not respond to bright light") {
                    !coordinator.readingDark && !coordinator.appDark && paintMatches(coordinator)
                }
                let stops = sensor.stops
                coordinator.setForeground(false)
                guard sensor.stops == stops + 1, coordinator.ambientDark == nil else {
                    throw CoreFailure(message: "Background did not release sensor")
                }
                sensor.level = nil
                coordinator.setForeground(true)
                try await wait("Missing readings did not become unavailable") { !coordinator.sensorAvailable }
                guard coordinator.readingDark == coordinator.systemDark && coordinator.value.readingSource == .ambient else {
                    throw CoreFailure(message: "Unavailable reading changed source or did not follow system")
                }
                sensor.level = 100
                try await wait("Sensor recovery did not update paint") { coordinator.ambientDark == false && paintMatches(coordinator) }
                coordinator.toggleReading()
                guard coordinator.value.readingMode == .dark && !coordinator.value.usesAmbient && coordinator.ambientDark == nil else {
                    throw CoreFailure(message: "Manual reading did not stop ambient sampling")
                }
                try await wait("Manual paint did not apply") { paintMatches(coordinator) }
                guard native()?.anchor == anchor, native()?.options.fontSize == font else {
                    throw CoreFailure(message: "Ambient transition changed anchor or typography")
                }
                guard AppearanceLinkage.decode(UserDefaults.standard.string(forKey: AppearanceLinkage.storageKey)) == coordinator.value else {
                    throw CoreFailure(message: "Preference not persisted")
                }
                report = ["passed": true, "physical": false, "productionHost": true,
                    "middleBandStartupLight": true, "dimRoomRemainsLight": true, "briefCoverRemainsLight": true,
                    "lightAndDarkPaint": true, "independentReading": true, "backgroundRelease": true,
                    "missingDataFallback": true, "recovery": true, "manualStopsSampling": true,
                    "anchorAndFontPreserved": true, "persisted": true]
            }
        } catch { report["error"] = String(describing: error); report["passed"] = false }
        save()
    }
}
#endif
