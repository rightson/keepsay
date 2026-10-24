// In-memory microphone recorder: AVAudioEngine → 16 kHz mono Float32
// (PROPOSAL §7.1, US-M2-08, INV-5).
//
// INV-5: audio never touches the disk. This file must not use AVAudioFile,
// AVAudioRecorder, ExtAudioFile or any URL/file API; samples live only in
// `SampleAccumulator` and are handed to the caller (then to core) on stop.

import AVFoundation
import Foundation

/// Bounded sample buffer. Pure logic, unit-tested.
public struct SampleAccumulator: Sendable {
    public let capacity: Int
    public private(set) var samples: [Float] = []

    public init(capacity: Int) {
        self.capacity = capacity
    }

    public var isFull: Bool { samples.count >= capacity }

    /// Appends up to the remaining capacity; returns how many were kept.
    @discardableResult
    public mutating func append(contentsOf chunk: UnsafeBufferPointer<Float>) -> Int {
        let room = max(0, capacity - samples.count)
        let n = min(room, chunk.count)
        if n > 0 { samples.append(contentsOf: chunk.prefix(n)) }
        return n
    }

    /// Hands the samples over and leaves the accumulator empty.
    public mutating func take() -> [Float] {
        let out = samples
        samples = []
        return out
    }

    /// Overwrites and drops the samples (cancel path, best effort: Swift
    /// arrays may have been copied by the runtime).
    public mutating func wipe() {
        for i in samples.indices { samples[i] = 0 }
        samples = []
    }

    /// Root-mean-square of a chunk, for the HUD level meter.
    public static func rms(_ chunk: UnsafeBufferPointer<Float>) -> Float {
        guard !chunk.isEmpty else { return 0 }
        var sum: Float = 0
        for s in chunk { sum += s * s }
        return (sum / Float(chunk.count)).squareRoot()
    }
}

public enum RecorderError: Error, Equatable {
    case noInputDevice
    case converterUnavailable
    case alreadyRecording
}

public final class PCMRecorder: @unchecked Sendable {
    public static let sampleRate: Double = 16_000
    /// 5 minutes; bounds memory (≈19 MB of f32).
    public static let maxSeconds: Double = 300

    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var buffer = SampleAccumulator(capacity: Int(PCMRecorder.sampleRate * PCMRecorder.maxSeconds))
    private var running = false

    /// Called on the audio thread with a 0...1 level; keep it cheap.
    public var onLevel: (@Sendable (Float) -> Void)?

    public init() {}

    public func start() throws {
        lock.lock()
        defer { lock.unlock() }
        guard !running else { throw RecorderError.alreadyRecording }

        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: [])
        try session.setActive(true, options: [])
        #endif

        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0, inFormat.channelCount > 0 else { throw RecorderError.noInputDevice }
        guard
            let outFormat = AVAudioFormat(
                commonFormat: .pcmFormatFloat32, sampleRate: Self.sampleRate, channels: 1, interleaved: false),
            let converter = AVAudioConverter(from: inFormat, to: outFormat)
        else { throw RecorderError.converterUnavailable }
        converter.downmix = true

        buffer = SampleAccumulator(capacity: Int(Self.sampleRate * Self.maxSeconds))
        let ratio = Self.sampleRate / inFormat.sampleRate
        input.installTap(onBus: 0, bufferSize: 4096, format: inFormat) { [weak self] pcm, _ in
            self?.consume(pcm, converter: converter, outFormat: outFormat, ratio: ratio)
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw error
        }
        running = true
    }

    /// Stops and returns the recorded samples; the recorder keeps no copy.
    public func stop() -> [Float] {
        halt()
        lock.lock()
        defer { lock.unlock() }
        return buffer.take()
    }

    /// Stops and discards the samples (Esc).
    public func cancel() {
        halt()
        lock.lock()
        defer { lock.unlock() }
        buffer.wipe()
    }

    /// The lock is not held while removing the tap: the tap block takes it.
    private func halt() {
        lock.lock()
        let wasRunning = running
        running = false
        lock.unlock()
        guard wasRunning else { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
        #endif
    }

    private func consume(
        _ pcm: AVAudioPCMBuffer, converter: AVAudioConverter, outFormat: AVAudioFormat, ratio: Double
    ) {
        let capacity = AVAudioFrameCount(Double(pcm.frameLength) * ratio) + 64
        guard let out = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        let status = converter.convert(to: out, error: &error) { _, inputStatus in
            if fed {
                inputStatus.pointee = .noDataNow
                return nil
            }
            fed = true
            inputStatus.pointee = .haveData
            return pcm
        }
        guard status != .error, let channel = out.floatChannelData?[0] else { return }
        let chunk = UnsafeBufferPointer(start: channel, count: Int(out.frameLength))
        let level = SampleAccumulator.rms(chunk)
        lock.lock()
        if running { buffer.append(contentsOf: chunk) }
        lock.unlock()
        onLevel?(min(1, level * 4))
    }
}
