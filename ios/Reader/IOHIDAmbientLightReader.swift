import Foundation
import Darwin

// Private iOS interface, intentionally included in both Debug and Release.
// Uses the same read-only path as the on-device Light Probe. No sensor property
// writes, private entitlements or screen-brightness inference are involved.
final class IOHIDAmbientLightReader: AmbientLightReading {
    private typealias Create = @convention(c) (CFAllocator?) -> UnsafeMutableRawPointer?
    private typealias Match = @convention(c) (UnsafeMutableRawPointer, CFDictionary) -> Void
    private typealias Services = @convention(c) (UnsafeMutableRawPointer) -> Unmanaged<CFArray>?
    private typealias Callback = @convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, UnsafeMutableRawPointer?) -> Void
    private typealias Register = @convention(c) (UnsafeMutableRawPointer, Callback, UnsafeMutableRawPointer?, UnsafeMutableRawPointer?) -> Void
    private typealias Unregister = @convention(c) (UnsafeMutableRawPointer) -> Void
    private typealias Schedule = @convention(c) (UnsafeMutableRawPointer, CFRunLoop, CFString) -> Void
    private typealias CopyEvent = @convention(c) (UnsafeRawPointer, Int64, Int32, Int64) -> Unmanaged<CFTypeRef>?
    private typealias EventType = @convention(c) (CFTypeRef) -> UInt32
    private typealias EventValue = @convention(c) (CFTypeRef, UInt32) -> Double
    private var library: UnsafeMutableRawPointer?
    private var client: UnsafeMutableRawPointer?
    private var services: CFArray?
    private var copyEvent: CopyEvent?
    private var eventType: EventType?
    private var eventValue: EventValue?
    private var unregister: Unregister?
    private var unschedule: Schedule?

    private func symbol<T>(_ name: String, as type: T.Type) -> T? {
        guard let library, let address = dlsym(library, name) else { return nil }
        return unsafeBitCast(address, to: type)
    }

    func start() -> Bool {
        stop()
        #if targetEnvironment(simulator)
        return false
        #else
        library = dlopen("/System/Library/Frameworks/IOKit.framework/IOKit", RTLD_NOW | RTLD_LOCAL)
        guard let create = symbol("IOHIDEventSystemClientCreate", as: Create.self),
              let match = symbol("IOHIDEventSystemClientSetMatching", as: Match.self),
              let copyServices = symbol("IOHIDEventSystemClientCopyServices", as: Services.self),
              let register = symbol("IOHIDEventSystemClientRegisterEventCallback", as: Register.self),
              let unregister = symbol("IOHIDEventSystemClientUnregisterEventCallback", as: Unregister.self),
              let schedule = symbol("IOHIDEventSystemClientScheduleWithRunLoop", as: Schedule.self),
              let unschedule = symbol("IOHIDEventSystemClientUnscheduleWithRunLoop", as: Schedule.self),
              let copyEvent = symbol("IOHIDServiceClientCopyEvent", as: CopyEvent.self),
              let eventType = symbol("IOHIDEventGetType", as: EventType.self),
              let eventValue = symbol("IOHIDEventGetFloatValue", as: EventValue.self),
              let client = create(kCFAllocatorDefault) else { stop(); return false }
        self.client = client
        match(client, ["PrimaryUsagePage": 0xff00, "PrimaryUsage": 4] as CFDictionary)
        services = copyServices(client)?.takeRetainedValue()
        guard let services, CFArrayGetCount(services) > 0 else { stop(); return false }
        self.copyEvent = copyEvent; self.eventType = eventType; self.eventValue = eventValue
        self.unregister = unregister; self.unschedule = unschedule
        // Poll for snapshots, as callbacks alone did not deliver readings in the probe.
        register(client, { _, _, _, _ in }, nil, nil)
        schedule(client, CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue)
        return true
        #endif
    }

    func read() -> Double? {
        guard let services, let copyEvent, let eventType, let eventValue else { return nil }
        for index in 0..<CFArrayGetCount(services) {
            guard let service = CFArrayGetValueAtIndex(services, index),
                  let event = copyEvent(service, 12, 0, 0)?.takeRetainedValue(),
                  eventType(event) == 12 else { continue }
            let level = eventValue(event, 12 << 16)
            if level.isFinite && level >= 0 { return level }
        }
        return nil
    }

    func stop() {
        if let client {
            unregister?(client)
            unschedule?(client, CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue)
            services = nil
            Unmanaged<CFTypeRef>.fromOpaque(client).release()
        }
        client = nil; services = nil
        copyEvent = nil; eventType = nil; eventValue = nil; unregister = nil; unschedule = nil
        if let library { dlclose(library) }
        library = nil
    }
    deinit { stop() }
}
