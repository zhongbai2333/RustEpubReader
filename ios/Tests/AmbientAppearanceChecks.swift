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
        var decision = AmbientAppearanceSwitch()
        precondition(decision.sample(0, at: 0) == nil)
        precondition(decision.sample(0, at: 2.9) == nil)
        precondition(decision.sample(0, at: 3) == true)
        precondition(decision.sample(25, at: 4) == true)
        precondition(decision.sample(100, at: 5) == true)
        precondition(decision.sample(100, at: 7.9) == true)
        precondition(decision.sample(100, at: 8) == false)
        precondition(decision.sample(0, at: 9) == false)
        precondition(decision.sample(.nan, at: 10) == false)
        precondition(decision.sample(0, at: 12) == false)
        precondition(decision.sample(0, at: 15) == true)
        for level in [Double.infinity, -Double.infinity, -1] { precondition(decision.sample(level, at: 16) == true) }
        decision.reset()
        precondition(decision.sample(15, at: 20) == nil)
        precondition(decision.sample(40, at: 24) == nil)
        precondition(decision.sample(100, at: 25) == nil)
        _ = decision.sample(0, at: 27)
        precondition(decision.sample(100, at: 28) == nil)
        precondition(decision.sample(100, at: 31) == false)

        let sensor = TestSensor()
        let running = AmbientAppearanceMonitor(reader: sensor)
        running.update(enabled: true, foreground: false, at: 0)
        precondition(sensor.starts == 0 && sensor.reads == 0)
        running.update(enabled: true, foreground: true, at: 1)
        precondition(sensor.starts == 1 && running.available && running.dark == nil)
        running.update(enabled: true, foreground: true, at: 4)
        precondition(running.dark == true)
        sensor.level = nil
        running.update(enabled: true, foreground: true, at: 5)
        precondition(running.dark == true)
        running.update(enabled: true, foreground: true, at: 9)
        precondition(running.dark == nil && !running.available)
        sensor.level = 100
        running.update(enabled: true, foreground: true, at: 10)
        precondition(running.available && running.dark == nil)
        running.update(enabled: true, foreground: true, at: 13)
        precondition(running.dark == false)
        running.update(enabled: true, foreground: false, at: 14)
        precondition(sensor.stops == 1 && running.dark == nil && !running.available)
        running.update(enabled: true, foreground: true, at: 15)
        precondition(sensor.starts == 2 && running.dark == nil)
        running.update(enabled: false, foreground: true, at: 16)
        let reads = sensor.reads
        running.update(enabled: false, foreground: true, at: 20)
        precondition(sensor.stops == 2 && sensor.reads == reads)
        sensor.succeeds = false
        running.update(enabled: true, foreground: true, at: 21)
        precondition(!running.available && running.dark == nil && sensor.starts == 3)
        running.update(enabled: true, foreground: true, at: 22)
        precondition(sensor.starts == 3)
        sensor.succeeds = true
        running.update(enabled: true, foreground: true, at: 26)
        precondition(sensor.starts == 4 && running.available)
        sensor.level = .nan
        running.update(enabled: true, foreground: true, at: 27)
        sensor.level = 100
        running.update(enabled: true, foreground: true, at: 29)
        precondition(running.dark == nil)
        running.update(enabled: true, foreground: true, at: 32)
        precondition(running.dark == false)
        print("Ambient appearance checks passed: dwell, hysteresis, invalid/stale data, lifecycle and recovery")
    }
}
