// PROPOSAL §7.2 / THREAT_MODEL T6: the result goes to the pasteboard with
// localOnly = true (never Universal Clipboard) and a 60 s expiration.
// The key never goes through the pasteboard (INV-1).

import UIKit
import UniformTypeIdentifiers

enum ClipboardPolicy {
    /// Seconds before iOS removes the item.
    static let lifetime: TimeInterval = 60

    /// Options for `UIPasteboard.setItems(_:options:)`.
    static func options(now: Date) -> [UIPasteboard.OptionsKey: Any] {
        [
            .localOnly: true,
            .expirationDate: now.addingTimeInterval(lifetime),
        ]
    }

    /// One plain-text item.
    static func items(for text: String) -> [[String: Any]] {
        [[UTType.utf8PlainText.identifier: text]]
    }

    /// Writes `text` with the policy options. Returns when it expires.
    @discardableResult
    static func copy(_ text: String, to pasteboard: UIPasteboard = .general, now: Date = Date()) -> Date {
        pasteboard.setItems(items(for: text), options: options(now: now))
        return now.addingTimeInterval(lifetime)
    }
}
