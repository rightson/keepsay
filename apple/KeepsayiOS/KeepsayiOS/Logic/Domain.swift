// App-side mirrors of the core's polish types (core/src/polish/modes.rs).
//
// They exist so the iOS logic and its tests do not depend on the generated
// UniFFI bindings. `CoreService` adapters translate between these and the
// bindings; nothing here ever holds the API key.

import Foundation

/// FR-4. `raw` never calls the API.
enum PolishMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case raw
    case light
    case polish
    case formal

    var id: String { rawValue }

    /// Segment label (mobile.html home / settings).
    var label: String {
        switch self {
        case .raw: return "原文"
        case .light: return "標點"
        case .polish: return "潤稿"
        case .formal: return "正式"
        }
    }

    var summary: String {
        switch self {
        case .raw: return "只轉寫，不連網"
        case .light: return "補標點、分段，不改字"
        case .polish: return "修順語句，保留原意"
        case .formal: return "改成正式書面語"
        }
    }
}

/// FR-8 v1 translation targets.
enum TargetLanguage: String, CaseIterable, Codable, Identifiable, Sendable {
    case en
    case zhTW = "zh-TW"
    case ja

    var id: String { rawValue }

    var label: String {
        switch self {
        case .en: return "English"
        case .zhTW: return "繁體中文（台灣）"
        case .ja: return "日本語"
        }
    }
}

/// The two user-facing actions (FR-2 iOS: in-app buttons and App Intents).
enum DictationAction: String, CaseIterable, Codable, Sendable {
    /// 潤飾: uses the selected mode.
    case polish
    /// 潤飾並翻譯: always polish + translate (FR-8).
    case polishAndTranslate

    var label: String {
        switch self {
        case .polish: return "潤飾"
        case .polishAndTranslate: return "潤飾並翻譯"
        }
    }
}

/// Mirror of `FallbackReason` (core/src/polish/modes.rs), plus the app-side
/// reason for "core not linked yet".
enum FallbackReason: Equatable, Sendable {
    case translationNeedsApi
    case noKey
    case keyInvalid
    case hardLimit
    case budgetExhausted
    case network
    case timeout
    case rateLimited
    case apiError
    case guardRejected
    case coreUnavailable

    /// User-facing line shown in the fallback banner. Never contains user
    /// content.
    var message: String {
        switch self {
        case .translationNeedsApi: return "翻譯需要 API，已輸出原文"
        case .noKey: return "未設定金鑰，已改用原文"
        case .keyInvalid: return "金鑰無效，請到設定更換・已改用原文"
        case .hardLimit: return "已達本月硬上限，已改用原文"
        case .budgetExhausted: return "帳號額度不足，已改用原文"
        case .network: return "無法連線，已改用原文"
        case .timeout: return "連線逾時，已改用原文"
        case .rateLimited: return "請求太頻繁，已改用原文"
        case .apiError: return "潤稿失敗，已改用原文"
        case .guardRejected: return "潤稿結果未通過檢查，已改用原文"
        case .coreUnavailable: return "潤稿元件尚未就緒，已改用原文"
        }
    }
}

/// Token usage of one API call (mirror of `budget::Usage`).
struct TokenUsage: Equatable, Sendable {
    var inputTokens: UInt64
    var outputTokens: UInt64
}

/// Mirror of `PolishResult`. `text` is user content: never log it.
struct PolishOutcome: Equatable, Sendable {
    var text: String
    var fellBackToRaw: Bool
    var fallbackReason: FallbackReason?
    var usage: TokenUsage?

    static func raw(_ transcript: String, reason: FallbackReason?) -> PolishOutcome {
        PolishOutcome(text: transcript, fellBackToRaw: reason != nil, fallbackReason: reason, usage: nil)
    }
}

/// Mirror of `BudgetStatus`.
struct BudgetSnapshot: Equatable, Sendable {
    var estimatedUSD: Double
    var softLimitUSD: Double?
    var hardLimitUSD: Double?

    static let empty = BudgetSnapshot(estimatedUSD: 0, softLimitUSD: nil, hardLimitUSD: nil)

    var softLimited: Bool { softLimitUSD.map { estimatedUSD >= $0 } ?? false }
    var hardLimited: Bool { hardLimitUSD.map { estimatedUSD >= $0 } ?? false }

    /// 0...1 fraction of the hard limit (or soft limit) for the mini bar.
    var fraction: Double {
        guard let cap = hardLimitUSD ?? softLimitUSD, cap > 0 else { return 0 }
        return min(1, max(0, estimatedUSD / cap))
    }
}

/// FR-6 soft limit: warn once per calendar month (local time), and not when
/// the hard limit already switched dictation to raw (that has its own
/// banner).
enum SoftLimitNotice {
    static func monthKey(_ date: Date, calendar: Calendar = .current) -> String {
        let c = calendar.dateComponents([.year, .month], from: date)
        return String(format: "%04d-%02d", c.year ?? 0, c.month ?? 0)
    }

    static func shouldWarn(
        budget: BudgetSnapshot, lastWarnedMonth: String?, now: Date, calendar: Calendar = .current
    ) -> Bool {
        budget.softLimited && !budget.hardLimited && lastWarnedMonth != monthKey(now, calendar: calendar)
    }

    static func message(_ budget: BudgetSnapshot) -> String {
        let cap = budget.softLimitUSD.map { String(format: "US$%.2f", $0) } ?? ""
        return "本月用量已達軟上限 \(cap)"
    }
}
