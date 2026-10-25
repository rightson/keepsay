// Non-secret settings (PROPOSAL §10). The source of truth is core's
// `config.rs` (`config.json`, schema version 1); this struct mirrors the
// fields the UI edits and is read and written through `KeepsayCore`, so the
// Swift side never writes its own settings file. The glossary is private
// data: it is never logged and `description` prints only its length.

import Foundation

public struct PriceTable: Equatable, Codable, Sendable {
    public var inputUSDPerMTok: Double
    public var outputUSDPerMTok: Double

    public init(inputUSDPerMTok: Double, outputUSDPerMTok: Double) {
        self.inputUSDPerMTok = inputUSDPerMTok
        self.outputUSDPerMTok = outputUSDPerMTok
    }
}

/// A Carbon-registrable combination: one key plus Carbon modifier mask.
public struct KeyCombo: Equatable, Codable, Sendable {
    /// Virtual key code (kVK_*).
    public var keyCode: UInt32
    /// Carbon modifier flags (cmdKey, shiftKey, optionKey, controlKey).
    public var carbonModifiers: UInt32

    public init(keyCode: UInt32, carbonModifiers: UInt32) {
        self.keyCode = keyCode
        self.carbonModifiers = carbonModifiers
    }

    // Carbon constants, duplicated so KeepsayKit stays importable on iOS.
    public static let cmdKey: UInt32 = 1 << 8
    public static let shiftKey: UInt32 = 1 << 9
    public static let optionKey: UInt32 = 1 << 11
    public static let controlKey: UInt32 = 1 << 12
    public static let kVKSpace: UInt32 = 0x31

    /// docs/changes/0004: ⌥Space / ⌥⇧Space.
    public static let fallbackPolish = KeyCombo(keyCode: kVKSpace, carbonModifiers: optionKey)
    public static let fallbackTranslate = KeyCombo(keyCode: kVKSpace, carbonModifiers: optionKey | shiftKey)
}

extension KeyCombo {
    private static let modifierNames: [(UInt32, String)] = [
        (controlKey, "Control"), (optionKey, "Option"), (shiftKey, "Shift"), (cmdKey, "Cmd"),
    ]

    /// The string stored in core's `config.json` `hotkeys` (docs/changes/0019),
    /// e.g. "Option+Shift+Space" or "Control+Option+VK6". Same style as
    /// core's `MACOS_FALLBACK_HOTKEYS`.
    public var configString: String {
        var parts = Self.modifierNames.filter { carbonModifiers & $0.0 != 0 }.map(\.1)
        parts.append(keyCode == Self.kVKSpace ? "Space" : "VK\(keyCode)")
        return parts.joined(separator: "+")
    }

    /// Parses `configString`; `nil` for anything else (the caller then uses
    /// the default combination).
    public init?(configString: String) {
        let parts = configString.split(separator: "+").map(String.init)
        guard let key = parts.last, parts.count >= 2 else { return nil }
        var mods: UInt32 = 0
        for name in parts.dropLast() {
            guard let m = Self.modifierNames.first(where: { $0.1 == name }) else { return nil }
            guard mods & m.0 == 0 else { return nil }
            mods |= m.0
        }
        let code: UInt32
        if key == "Space" {
            code = Self.kVKSpace
        } else if key.hasPrefix("VK"), let n = UInt32(key.dropFirst(2)), n < 128 {
            code = n
        } else {
            return nil
        }
        self.init(keyCode: code, carbonModifiers: mods)
    }
}

public struct AppSettings: Equatable, Codable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    public var mode: DictationMode
    public var translateTo: TranslationTarget
    /// Selected STT model id from models/manifest.json.
    public var sttModel: String?
    public var glossary: [String]
    public var prices: PriceTable
    public var softLimitUSD: Double?
    public var hardLimitUSD: Double?
    /// macOS: register the Carbon combinations even when the fn tap works.
    public var alwaysEnableFallbackHotkeys: Bool
    public var fallbackPolish: KeyCombo
    public var fallbackTranslate: KeyCombo

    public init(
        mode: DictationMode = .polish,
        translateTo: TranslationTarget = .en,
        sttModel: String? = "whisper-small",
        glossary: [String] = [],
        prices: PriceTable = PriceTable(inputUSDPerMTok: 0.10, outputUSDPerMTok: 0.50),
        softLimitUSD: Double? = 5,
        hardLimitUSD: Double? = 10,
        alwaysEnableFallbackHotkeys: Bool = false,
        fallbackPolish: KeyCombo = .fallbackPolish,
        fallbackTranslate: KeyCombo = .fallbackTranslate
    ) {
        self.mode = mode
        self.translateTo = translateTo
        self.sttModel = sttModel
        self.glossary = glossary
        self.prices = prices
        self.softLimitUSD = softLimitUSD
        self.hardLimitUSD = hardLimitUSD
        self.alwaysEnableFallbackHotkeys = alwaysEnableFallbackHotkeys
        self.fallbackPolish = fallbackPolish
        self.fallbackTranslate = fallbackTranslate
    }

    /// FR-2: the polish hotkey uses the default mode; the translate hotkey
    /// is always polish + translate.
    public func polishRequest(for action: HotkeyAction) -> PolishRequest {
        switch action {
        case .polish:
            PolishRequest(mode: mode, translateTo: nil, glossary: glossary)
        case .polishAndTranslate:
            PolishRequest(mode: .polish, translateTo: translateTo, glossary: glossary)
        }
    }

    /// Adds a glossary term; trims whitespace, ignores empty and duplicate
    /// entries. Returns whether the list changed.
    @discardableResult
    public mutating func addGlossaryTerm(_ term: String) -> Bool {
        let t = term.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty, !glossary.contains(t) else { return false }
        glossary.append(t)
        return true
    }

    public var description: String {
        "AppSettings(mode: \(mode.rawValue), translate_to: \(translateTo.rawValue), glossary_len: \(glossary.count))"
    }

    public var debugDescription: String { description }
}
