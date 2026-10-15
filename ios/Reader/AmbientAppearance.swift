import Foundation

// The IOHID value is kept in the sensor's native scale, not advertised as calibrated lux.
struct AmbientAppearanceSwitch {
    private(set) var dark: Bool?
    private var candidate: Bool?
    private var since: TimeInterval = 0

    mutating func sample(_ level: Double, at now: TimeInterval) -> Bool? {
        guard level.isFinite, level >= 0 else { interrupt(); return dark }
        let target: Bool?
        // Require deeper darkness for longer; ordinary dim indoor light should stay light.
        if level < 5 { target = true }
        else if level > 12 { target = false }
        // On startup, a valid middle-band reading must settle instead of indefinitely
        // inheriting a dark system theme. Once settled, retain hysteresis in this band.
        else { target = dark ?? false }
        guard let target, target != dark else { interrupt(); return dark }
        if candidate != target { candidate = target; since = now }
        let dwell: TimeInterval = target ? 5 : 3
        if now - since >= dwell { dark = target; interrupt() }
        return dark
    }
    mutating func interrupt() { candidate = nil }
    mutating func reset() { self = Self() }
}

protocol AmbientLightReading: AnyObject {
    func start() -> Bool
    func read() -> Double?
    func stop()
}

// Called from the foreground appearance task. Sensor decisions are never persisted.
final class AmbientAppearanceMonitor {
    private let reader: AmbientLightReading
    private var opened = false
    private var openedAt: TimeInterval = 0
    private var retryAt: TimeInterval = 0
    private var lastValidAt: TimeInterval?
    private var decision = AmbientAppearanceSwitch()
    private(set) var dark: Bool?
    private(set) var available = false
    private(set) var level: Double?

    init(reader: AmbientLightReading) { self.reader = reader }

    func update(enabled: Bool, foreground: Bool, at now: TimeInterval) {
        guard enabled && foreground else { stop(); return }
        if !opened {
            guard now >= retryAt else { return }
            opened = reader.start()
            available = opened
            retryAt = now + 5
            openedAt = now
            guard opened else { return }
        }
        if let sample = reader.read(), sample.isFinite, sample >= 0 {
            level = sample
            lastValidAt = now
            available = true
            dark = decision.sample(sample, at: now)
        } else {
            level = nil
            decision.interrupt()
            if now - (lastValidAt ?? openedAt) >= 5 {
                available = false
                dark = nil
                decision.reset()
            }
        }
    }

    private func stop() {
        if opened { reader.stop() }
        opened = false; available = false; dark = nil; level = nil
        lastValidAt = nil; retryAt = 0; decision.reset()
    }
    deinit { if opened { reader.stop() } }
}
