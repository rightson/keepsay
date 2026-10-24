// Swift-side mirrors of the keepsay-core FFI types (PROPOSAL §6.1).
//
// They are deliberately separate from the UniFFI-generated types so the UI
// and its tests do not depend on the Rust build, and so the generated names
// (`Mode`, `PolishOptions`, ...) never collide with these. `UniffiCore`
// converts between the two.
//
// Rule (INV-2): nothing here carries the API key. Types that hold user
// content (transcripts, output, glossary) print only lengths.

import Foundation

/// FR-4 polish modes. `raw` never calls the API.
public enum DictationMode: String, CaseIterable, Codable, Sendable {
    case raw, light, polish, formal

    /// Labels from docs/mockups/desktop.html.
    public var label: String {
        switch self {
        case .raw: "原文"
        case .light: "標點"
        case .polish: "潤稿"
        case .formal: "正式"
        }
    }

    public var detail: String {
        switch self {
        case .raw: "只轉寫，不連網"
        case .light: "補標點、分段，不改字"
        case .polish: "修順語句，保留原意"
        case .formal: "改成正式書面語"
        }
    }

    public var needsAPI: Bool { self != .raw }
}

/// FR-8 translation targets. Raw values match core's serde names.
public enum TranslationTarget: String, CaseIterable, Codable, Sendable {
    case en
    case zhTW = "zh-TW"
    case ja

    public var label: String {
        switch self {
        case .en: "English"
        case .zhTW: "繁體中文（台灣）"
        case .ja: "日本語"
        }
    }
}

/// Which hotkey started the recording (FR-2: decided at start, not at stop).
public enum HotkeyAction: Hashable, Sendable {
    case polish
    case polishAndTranslate
}

/// Mirrors core `FallbackReason`.
public enum FallbackCause: Equatable, Sendable {
    case translationNeedsAPI
    case noKey
    case keyInvalid
    case hardLimit
    case budgetExhausted
    case network
    case timeout
    case rateLimited
    case apiError
    case guardRejected

    /// HUD text (FR-4, FR-8, US-M2-13).
    public var message: String {
        switch self {
        case .translationNeedsAPI: "翻譯需要 API，已輸出原文"
        case .noKey: "未設定金鑰，已輸出原文"
        case .keyInvalid: "金鑰無效或已撤銷，已輸出原文"
        case .hardLimit: "已達本月硬上限，已輸出原文"
        case .budgetExhausted: "帳戶額度用盡，已輸出原文"
        case .network: "無法連線，已輸出原文"
        case .timeout: "連線逾時，已輸出原文"
        case .rateLimited: "請求過多，已輸出原文"
        case .apiError: "潤稿失敗，已輸出原文"
        case .guardRejected: "潤稿結果未通過檢查，已輸出原文"
        }
    }
}

/// Input to `polish`. `glossary` is private data (§10).
public struct PolishRequest: Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    public var mode: DictationMode
    public var translateTo: TranslationTarget?
    public var glossary: [String]

    public init(mode: DictationMode, translateTo: TranslationTarget?, glossary: [String]) {
        self.mode = mode
        self.translateTo = translateTo
        self.glossary = glossary
    }

    public var needsAPI: Bool { mode.needsAPI || translateTo != nil }

    public var description: String {
        "PolishRequest(mode: \(mode.rawValue), translate: \(translateTo?.rawValue ?? "none"), glossary_len: \(glossary.count))"
    }

    public var debugDescription: String { description }
}

public struct TokenUsage: Equatable, Sendable {
    public var inputTokens: UInt64
    public var outputTokens: UInt64

    public init(inputTokens: UInt64, outputTokens: UInt64) {
        self.inputTokens = inputTokens
        self.outputTokens = outputTokens
    }
}

/// Mirrors core `PolishResult`. `text` is user content.
public struct PolishOutcome: Equatable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    public var text: String
    public var fellBackToRaw: Bool
    public var fallbackCause: FallbackCause?
    public var usage: TokenUsage?

    public init(text: String, fellBackToRaw: Bool, fallbackCause: FallbackCause?, usage: TokenUsage?) {
        self.text = text
        self.fellBackToRaw = fellBackToRaw
        self.fallbackCause = fallbackCause
        self.usage = usage
    }

    /// The raw transcript unchanged, as core does for every failure.
    public static func raw(_ transcript: String, cause: FallbackCause?) -> PolishOutcome {
        PolishOutcome(text: transcript, fellBackToRaw: cause != nil, fallbackCause: cause, usage: nil)
    }

    public var description: String {
        "PolishOutcome(text_len: \(text.count), fell_back: \(fellBackToRaw), cause: \(String(describing: fallbackCause)))"
    }

    public var debugDescription: String { description }
}

/// Mirrors core `BudgetStatus`.
public struct BudgetSnapshot: Equatable, Sendable {
    public var usage: TokenUsage
    public var estimatedUSD: Double
    public var softLimitUSD: Double?
    public var hardLimitUSD: Double?

    public init(usage: TokenUsage, estimatedUSD: Double, softLimitUSD: Double?, hardLimitUSD: Double?) {
        self.usage = usage
        self.estimatedUSD = estimatedUSD
        self.softLimitUSD = softLimitUSD
        self.hardLimitUSD = hardLimitUSD
    }

    public static let empty = BudgetSnapshot(
        usage: TokenUsage(inputTokens: 0, outputTokens: 0), estimatedUSD: 0, softLimitUSD: nil, hardLimitUSD: nil)

    public var softLimited: Bool { softLimitUSD.map { estimatedUSD >= $0 } ?? false }
    public var hardLimited: Bool { hardLimitUSD.map { estimatedUSD >= $0 } ?? false }
}

/// One entry of models/manifest.json plus local state.
public struct ModelInfo: Identifiable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var sizeBytes: UInt64
    public var installed: Bool

    public init(id: String, name: String, sizeBytes: UInt64, installed: Bool) {
        self.id = id
        self.name = name
        self.sizeBytes = sizeBytes
        self.installed = installed
    }
}

/// Mirrors core `CoreError`. Never carries content (§6.9).
public enum CoreFailure: Error, Equatable, Sendable {
    case keyInvalid
    case keyFormat
    case keyNotFound
    case vaultUnavailable
    case disallowedHost
    case network
    case timeout
    case rateLimited
    case budgetExhausted
    case apiStatus(UInt16)
    case invalidResponse
    case modelNotAllowed
    case guardRejected
    case hardLimitReached
    case audioFormat
    case stt
    case modelUnknown
    case modelHash
    case modelTooLarge
    case config
    case io
    case unsupported
    /// This build has no Rust core linked (development builds before
    /// apple/scripts/build-core.sh ran).
    case coreNotLinked

    /// Short zh-TW text for settings panes and alerts.
    public var message: String {
        switch self {
        case .keyInvalid: "金鑰無效或已撤銷"
        case .keyFormat: "金鑰格式不對（應以 sk-ant- 開頭）"
        case .keyNotFound: "尚未設定金鑰"
        case .vaultUnavailable: "無法存取鑰匙圈"
        case .disallowedHost: "連線目的地不在允許清單"
        case .network: "無法連線"
        case .timeout: "連線逾時"
        case .rateLimited: "請求過多，請稍後再試"
        case .budgetExhausted: "帳戶額度用盡"
        case .apiStatus(let code): "API 回應錯誤（\(code)）"
        case .invalidResponse: "API 回應格式不符"
        case .modelNotAllowed: "模型不在允許清單"
        case .guardRejected: "潤稿結果未通過檢查"
        case .hardLimitReached: "已達本月硬上限"
        case .audioFormat: "音訊格式不支援"
        case .stt: "語音辨識失敗"
        case .modelUnknown: "未知的語音模型"
        case .modelHash: "模型檔 SHA-256 不符，已刪除"
        case .modelTooLarge: "模型檔大小超過清單上限"
        case .config: "設定檔無法讀取"
        case .io: "檔案存取失敗"
        case .unsupported: "此版本無法使用安全儲存區（未簽章的開發版）"
        case .coreNotLinked: "此開發版未連結 keepsay-core"
        }
    }
}
