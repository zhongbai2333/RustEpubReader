import Foundation

private final class TestSensor: AmbientLightReading {
    var succeeds = true
    var level: Double? = 0
    var starts = 0, reads = 0, stops = 0
    func start() -> Bool { starts += 1; return succeeds }
    func read() -> Double? { reads += 1; return level }
    func stop() { stops += 1 }
}

@main enum AmbientAppearanceChecks {
    static func main() {
        // Readings that the former <15 rule treated as dark now remain light.
        for level in [5.0, 7, 10, 12, 14, 25, 40, 100] {
            var decision = AmbientAppearanceSwitch()
            precondition(decision.sample(level, at: 0) == nil)
            precondition(decision.sample(level, at: 2.9) == nil)
            precondition(decision.sample(level, at: 3) == false)
            precondition(decision.sample(level, at: 30) == false)
        }

        var decision = AmbientAppearanceSwitch()
        precondition(decision.sample(4, at: 0) == nil)
        precondition(decision.sample(4, at: 4.9) == nil)
        precondition(decision.sample(4, at: 5) == true)
        // The middle band retains a settled dark appearance, including both boundaries.
        for (time, level) in [5.0, 7, 12].enumerated() {
            precondition(decision.sample(level, at: 6 + Double(time)) == true)
        }
        precondition(decision.sample(13, at: 9) == true)
        precondition(decision.sample(13, at: 11.9) == true)
        precondition(decision.sample(13, at: 12) == false)
        // Brief covering and a return to dim indoor light cancel the pending dark switch.
        precondition(decision.sample(0, at: 13) == false)
        precondition(decision.sample(0, at: 17.9) == false)
        precondition(decision.sample(7, at: 18) == false)
        precondition(decision.sample(0, at: 19) == false)
        precondition(decision.sample(0, at: 23.9) == false)
        precondition(decision.sample(0, at: 24) == true)
        // Invalid readings interrupt either pending transition without replacing the decision.
        precondition(decision.sample(100, at: 25) == true)
        precondition(decision.sample(.nan, at: 26) == true)
        precondition(decision.sample(100, at: 28) == true)
        precondition(decision.sample(100, at: 31) == false)
        precondition(decision.sample(0, at: 32) == false)
        for (offset, level) in [Double.infinity, -Double.infinity, -1].enumerated() {
            precondition(decision.sample(level, at: 33 + Double(offset)) == false)
        }
        precondition(decision.sample(0, at: 36) == false)
        precondition(decision.sample(0, at: 40.9) == false)
        precondition(decision.sample(0, at: 41) == true)
        decision.reset()
        precondition(decision.dark == nil)
        precondition(decision.sample(7, at: 50) == nil)
        precondition(decision.sample(7, at: 53) == false)

        let sensor = TestSensor()
        let running = AmbientAppearanceMonitor(reader: sensor)
        running.update(enabled: true, foreground: false, at: 0)
        precondition(sensor.starts == 0 && sensor.reads == 0)
        running.update(enabled: true, foreground: true, at: 1)
        precondition(sensor.starts == 1 && running.available && running.dark == nil)
        running.update(enabled: true, foreground: true, at: 6)
        precondition(running.dark == true)
        sensor.level = nil
        running.update(enabled: true, foreground: true, at: 7)
        precondition(running.dark == true)
        running.update(enabled: true, foreground: true, at: 11)
        precondition(running.dark == nil && !running.available)
        sensor.level = 100
        running.update(enabled: true, foreground: true, at: 12)
        precondition(running.available && running.dark == nil)
        running.update(enabled: true, foreground: true, at: 15)
        precondition(running.dark == false)
        running.update(enabled: true, foreground: false, at: 16)
        precondition(sensor.stops == 1 && running.dark == nil && !running.available)
        running.update(enabled: true, foreground: true, at: 17)
        precondition(sensor.starts == 2 && running.dark == nil)
        running.update(enabled: false, foreground: true, at: 18)
        let reads = sensor.reads
        running.update(enabled: false, foreground: true, at: 22)
        precondition(sensor.stops == 2 && sensor.reads == reads)
        sensor.succeeds = false
        running.update(enabled: true, foreground: true, at: 23)
        precondition(!running.available && running.dark == nil && sensor.starts == 3)
        running.update(enabled: true, foreground: true, at: 24)
        precondition(sensor.starts == 3)
        sensor.succeeds = true
        running.update(enabled: true, foreground: true, at: 28)
        precondition(sensor.starts == 4 && running.available)
        sensor.level = .nan
        running.update(enabled: true, foreground: true, at: 29)
        sensor.level = 100
        running.update(enabled: true, foreground: true, at: 31)
        precondition(running.dark == nil)
        running.update(enabled: true, foreground: true, at: 34)
        precondition(running.dark == false)
        print("Ambient appearance checks passed: dim-room startup, deeper-dark threshold, asymmetric dwell, hysteresis, invalid/stale data, lifecycle and recovery")
    }
}
