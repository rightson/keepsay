// FR-2 (iOS): two App Intents for Shortcuts and the Action Button.
//
// Recording needs the app in the foreground (microphone), so both intents
// open the app and toggle dictation: the first run starts recording, a
// second run stops and processes. The intents return no transcript or
// output to Shortcuts: results only go to the local, expiring pasteboard
// (T6), never into a Shortcuts variable that other actions could forward.

import AppIntents

struct PolishDictationIntent: AppIntent {
    static let title: LocalizedStringResource = "潤飾"
    static let description = IntentDescription("開始或結束聽寫。結束後在手機上轉寫並潤飾，結果複製到剪貼簿（60 秒後清除）。")
    static let openAppWhenRun: Bool = true

    @MainActor
    func perform() async throws -> some IntentResult {
        AppModel.shared.handleIntent(.polish)
        return .result()
    }
}

struct PolishAndTranslateDictationIntent: AppIntent {
    static let title: LocalizedStringResource = "潤飾並翻譯"
    static let description = IntentDescription("開始或結束聽寫。結束後潤飾並翻譯成設定中的目標語言，結果複製到剪貼簿（60 秒後清除）。")
    static let openAppWhenRun: Bool = true

    @MainActor
    func perform() async throws -> some IntentResult {
        AppModel.shared.handleIntent(.polishAndTranslate)
        return .result()
    }
}

struct KeepsayShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: PolishDictationIntent(),
            phrases: ["用 \(.applicationName) 潤飾", "\(.applicationName) 聽寫"],
            shortTitle: "潤飾",
            systemImageName: "mic")
        AppShortcut(
            intent: PolishAndTranslateDictationIntent(),
            phrases: ["用 \(.applicationName) 潤飾並翻譯", "\(.applicationName) 翻譯"],
            shortTitle: "潤飾並翻譯",
            systemImageName: "globe")
    }
}

extension AppModel {
    /// Intents run before onboarding is finished too; then they only open
    /// the app on the onboarding screen.
    func handleIntent(_ action: DictationAction) {
        guard onboarding == .finished else { return }
        trigger(action)
    }
}
