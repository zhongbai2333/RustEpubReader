import SwiftUI
import UIKit

@MainActor final class AppearanceCoordinator: ObservableObject {
    @Published var value: AppearanceLinkage {
        willSet { transition(app: newValue.appDark(system: systemDark, ambient: ambientDark), reading: newValue.readingDark(system: systemDark, ambient: ambientDark)) }
        didSet {
            defaults.set(value.encoded, forKey: AppearanceLinkage.storageKey)
            refreshAmbient()
        }
    }
    @Published var systemDark = UIScreen.main.traitCollection.userInterfaceStyle == .dark {
        willSet { transition(app: value.appDark(system: newValue, ambient: ambientDark), reading: value.readingDark(system: newValue, ambient: ambientDark)) }
    }
    @Published private(set) var ambientDark: Bool? {
        willSet { transition(app: value.appDark(system: systemDark, ambient: newValue), reading: value.readingDark(system: systemDark, ambient: newValue)) }
    }
    private var fadingAppearance: UIView?
    private func transition(app: Bool, reading: Bool) {
        guard app != appDark || reading != readingDark, !UIAccessibility.isReduceMotionEnabled,
              let window = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).filter({ $0.activationState == .foregroundActive })
                .flatMap(\.windows).first(where: \.isKeyWindow),
              let frozen = window.snapshotView(afterScreenUpdates: false) else { return }
        fadingAppearance?.removeFromSuperview()
        frozen.frame = window.bounds
        frozen.isUserInteractionEnabled = false
        frozen.accessibilityElementsHidden = true
        fadingAppearance = frozen
        window.addSubview(frozen)
        DispatchQueue.main.async { [weak window] in
            window?.layoutIfNeeded()
            UIView.animate(withDuration: 0.6, delay: 0, options: [.curveEaseInOut, .allowUserInteraction]) {
                frozen.alpha = 0
            } completion: { [weak self] _ in
                frozen.removeFromSuperview()
                if self?.fadingAppearance === frozen { self?.fadingAppearance = nil }
            }
        }
    }
    @Published private(set) var sensorAvailable = false
    private(set) var ambientLevel: Double?
    private let ambient: AmbientAppearanceMonitor
    private var foreground = false
    private let defaults: UserDefaults
    init(defaults: UserDefaults = .standard, reader: AmbientLightReading = IOHIDAmbientLightReader()) {
        self.defaults = defaults
        ambient = AmbientAppearanceMonitor(reader: reader)
        let existing = defaults.object(forKey: ReadingAppearance.storageKey) != nil || defaults.object(forKey: "preview.theme") != nil
        ReadingAppearance.migrate(defaults)
        value = AppearanceLinkage.decode(defaults.string(forKey: AppearanceLinkage.storageKey)) ??
            (existing ? .migrate(reading: ReadingAppearance.decode(defaults.string(forKey: ReadingAppearance.storageKey) ?? "").mode) : .firstRun)
        defaults.set(value.encoded, forKey: AppearanceLinkage.storageKey)
    }
    var appDark: Bool { value.appDark(system: systemDark, ambient: ambientDark) }
    var readingDark: Bool { value.readingDark(system: systemDark, ambient: ambientDark) }
    // Read the system signal outside the app's overridden presentation traits.
    func refreshSystemAppearance() {
        let dark = UIScreen.main.traitCollection.userInterfaceStyle == .dark
        if systemDark != dark { systemDark = dark }
        refreshAmbient()
    }
    func setForeground(_ active: Bool) { foreground = active; refreshAmbient() }
    private func refreshAmbient() {
        ambient.update(enabled: value.usesAmbient, foreground: foreground, at: ProcessInfo.processInfo.systemUptime)
        if ambientDark != ambient.dark { ambientDark = ambient.dark }
        if sensorAvailable != ambient.available { sensorAvailable = ambient.available }
        ambientLevel = ambient.level
    }
    func toggleAppLink() { value.toggleAppLink(system: systemDark, ambient: ambientDark) }
    func toggleReadingLink() { value.toggleReadingLink(system: systemDark, ambient: ambientDark) }
    func toggleReading() { value.toggleReading(system: systemDark, ambient: ambientDark) }
}
