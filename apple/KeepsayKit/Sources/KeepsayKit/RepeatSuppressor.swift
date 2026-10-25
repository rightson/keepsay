// Turns a stream of hotkey events (including key auto-repeat) into toggle
// triggers without reading the auto-repeat field in the event tap (the tap
// callback reads only the keycode and fn/Shift, THREAT_MODEL T7).
//
// An event fires only if no event of the same hotkey arrived within
// `quietInterval` before it. Auto-repeat events arrive every ~30–100 ms
// while the key is held, so holding the key fires exactly once; a deliberate
// second press after releasing fires again.

import Foundation

public struct RepeatSuppressor: Sendable {
    public var quietInterval: TimeInterval
    private var lastEvent: TimeInterval?

    public init(quietInterval: TimeInterval = 0.25) {
        self.quietInterval = quietInterval
    }

    /// `now` is a monotonic timestamp in seconds.
    public mutating func shouldFire(at now: TimeInterval) -> Bool {
        defer { lastEvent = now }
        guard let last = lastEvent else { return true }
        return now - last >= quietInterval
    }

    public mutating func reset() {
        lastEvent = nil
    }
}
