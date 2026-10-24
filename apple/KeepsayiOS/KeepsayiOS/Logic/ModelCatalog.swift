// STT models offered on iOS (ids and sizes from models/manifest.json; the
// core owns URLs and SHA-256, INV-7). US-M4a-04: recommend by device memory.

import Foundation

struct SttModelInfo: Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var sizeBytes: Int64
    var summary: String

    var sizeLabel: String {
        ByteCountFormatter.string(fromByteCount: sizeBytes, countStyle: .file)
    }
}

enum ModelCatalog {
    static let base = SttModelInfo(
        id: "whisper-base", name: "Whisper base", sizeBytes: 147_951_465,
        summary: "最省記憶體，適合較舊的機型。準確度較低。")
    static let small = SttModelInfo(
        id: "whisper-small", name: "Whisper small", sizeBytes: 487_601_967,
        summary: "中文準確度與速度的平衡點。建議在 Wi‑Fi 下載。")
    static let largeTurbo = SttModelInfo(
        id: "whisper-large-v3-turbo", name: "Whisper large-v3-turbo", sizeBytes: 1_624_555_275,
        summary: "最準確，需要較新的機型（8 GB 記憶體以上）。")

    static let all: [SttModelInfo] = [base, small, largeTurbo]

    static func info(id: String) -> SttModelInfo? { all.first { $0.id == id } }

    /// Recommendation by physical memory. Thresholds are initial values;
    /// measured peaks go to docs/changes (US-M4a-04).
    static func recommended(physicalMemoryBytes: UInt64) -> SttModelInfo {
        let gib = Double(physicalMemoryBytes) / 1_073_741_824
        if gib >= 7.5 { return largeTurbo }
        if gib >= 3.5 { return small }
        return base
    }
}
