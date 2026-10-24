// iOS audio capture. INV-5: samples live only in memory (an in-process
// buffer), are never written to disk, and are dropped after transcription.
//
// The shared AVAudioEngine recorder lives in KeepsayKit (M2). This file owns
// what is iOS-only: the AVAudioSession and the microphone permission. Until
// KeepsayKit is linked, `MemoryRecorder` is a minimal in-memory recorder
// with the same contract (16 kHz mono f32).

import AVFoundation

protocol AudioCapturing: AnyObject {
    func requestPermission() async -> Bool
    func start(level: @escaping @Sendable (Float) -> Void) throws
    /// Stops and hands over the samples; the recorder keeps no copy.
    func stop() -> [Float]
    func cancel()
    var sampleRate: UInt32 { get }
}

enum AudioCaptureError: Error {
    case sessionUnavailable
    case formatUnavailable
}

final class MemoryRecorder: AudioCapturing, @unchecked Sendable {
    let sampleRate: UInt32 = 16_000

    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var samples: [Float] = []
    private var converter: AVAudioConverter?

    func requestPermission() async -> Bool {
        await AVAudioApplication.requestRecordPermission()
    }

    func start(level: @escaping @Sendable (Float) -> Void) throws {
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.record, mode: .measurement, options: [])
            try session.setActive(true, options: [])
        } catch {
            throw AudioCaptureError.sessionUnavailable
        }

        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard
            inFormat.sampleRate > 0,
            let outFormat = AVAudioFormat(
                commonFormat: .pcmFormatFloat32, sampleRate: Double(sampleRate), channels: 1, interleaved: false),
            let converter = AVAudioConverter(from: inFormat, to: outFormat)
        else {
            throw AudioCaptureError.formatUnavailable
        }
        self.converter = converter
        lock.withLock { samples.removeAll(keepingCapacity: true) }

        input.installTap(onBus: 0, bufferSize: 4096, format: inFormat) { [weak self] buffer, _ in
            self?.append(buffer, outFormat: outFormat, level: level)
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw AudioCaptureError.sessionUnavailable
        }
    }

    private func append(_ buffer: AVAudioPCMBuffer, outFormat: AVAudioFormat, level: @Sendable (Float) -> Void) {
        guard let converter else { return }
        let ratio = outFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio + 32)
        guard let out = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if fed {
                status.pointee = .noDataNow
                return nil
            }
            fed = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let channel = out.floatChannelData?[0] else { return }
        let chunk = UnsafeBufferPointer(start: channel, count: Int(out.frameLength))
        var peak: Float = 0
        for s in chunk { peak = max(peak, abs(s)) }
        lock.withLock { samples.append(contentsOf: chunk) }
        level(peak)
    }

    func stop() -> [Float] {
        teardown()
        return lock.withLock {
            let out = samples
            samples = []
            return out
        }
    }

    func cancel() {
        teardown()
        lock.withLock { samples = [] }
    }

    private func teardown() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        converter = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
    }
}
