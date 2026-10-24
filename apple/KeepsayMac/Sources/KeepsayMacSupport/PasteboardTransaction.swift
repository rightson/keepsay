// Clipboard fallback for insertion (PROPOSAL §7.1, THREAT_MODEL T6,
// US-M2-11): back up every pasteboard item, write the text with the
// nspasteboard.org Concealed + Transient markers, paste, then restore the
// backup 300 ms later.
//
// The model is separated from NSPasteboard (`PasteboardIO`) so the backup /
// restore rules are unit-tested. Snapshots hold user data: `description`
// prints only counts and byte lengths, and nothing here logs.

import AppKit

public struct PasteboardEntry: Equatable, CustomStringConvertible, CustomDebugStringConvertible {
    public var type: String
    public var data: Data

    public init(type: String, data: Data) {
        self.type = type
        self.data = data
    }

    public var description: String { "PasteboardEntry(type: \(type), bytes: \(data.count))" }
    public var debugDescription: String { description }
}

public struct PasteboardSnapshot: Equatable, CustomStringConvertible, CustomDebugStringConvertible {
    /// One inner array per pasteboard item, all of its types in order.
    public var items: [[PasteboardEntry]]

    public init(items: [[PasteboardEntry]]) {
        self.items = items
    }

    public var description: String {
        "PasteboardSnapshot(items: \(items.count), bytes: \(items.joined().reduce(0) { $0 + $1.data.count }))"
    }

    public var debugDescription: String { description }
}

public protocol PasteboardIO: AnyObject {
    var changeCount: Int { get }
    func snapshot() -> PasteboardSnapshot
    /// Clears the pasteboard and writes one item; returns the new changeCount.
    func write(item: [PasteboardEntry]) -> Int
    /// Clears the pasteboard and writes `snapshot` back.
    func restore(_ snapshot: PasteboardSnapshot)
}

public enum PasteboardMarkers {
    /// http://nspasteboard.org — clipboard managers skip these items.
    public static let concealed = "org.nspasteboard.ConcealedType"
    public static let transient = "org.nspasteboard.TransientType"
    public static let plainText = NSPasteboard.PasteboardType.string.rawValue

    /// The item written for a paste: UTF-8 text plus both markers.
    public static func item(for text: String) -> [PasteboardEntry] {
        [
            PasteboardEntry(type: plainText, data: Data(text.utf8)),
            PasteboardEntry(type: concealed, data: Data()),
            PasteboardEntry(type: transient, data: Data()),
        ]
    }
}

/// One paste. `begin` backs up and writes; `finish` restores, unless the
/// user (or another app) changed the pasteboard in between: then their newer
/// content wins and the backup is dropped.
public final class PasteboardTransaction {
    public static let restoreDelay: TimeInterval = 0.3

    private let io: PasteboardIO
    private var backup: PasteboardSnapshot?
    private var ourChangeCount: Int?

    public init(io: PasteboardIO) {
        self.io = io
    }

    public func begin(text: String) {
        backup = io.snapshot()
        ourChangeCount = io.write(item: PasteboardMarkers.item(for: text))
    }

    public enum Outcome: Equatable {
        case restored
        /// Someone else wrote to the pasteboard after us.
        case skippedChangedByOthers
        case notStarted
    }

    @discardableResult
    public func finish() -> Outcome {
        defer {
            backup = nil
            ourChangeCount = nil
        }
        guard let backup, let ours = ourChangeCount else { return .notStarted }
        guard io.changeCount == ours else { return .skippedChangedByOthers }
        io.restore(backup)
        return .restored
    }
}

/// `PasteboardIO` over `NSPasteboard.general`.
public final class SystemPasteboard: PasteboardIO {
    private let pb: NSPasteboard

    public init(_ pb: NSPasteboard = .general) {
        self.pb = pb
    }

    public var changeCount: Int { pb.changeCount }

    public func snapshot() -> PasteboardSnapshot {
        let items = (pb.pasteboardItems ?? []).map { item in
            item.types.compactMap { type in item.data(forType: type).map { PasteboardEntry(type: type.rawValue, data: $0) } }
        }
        return PasteboardSnapshot(items: items)
    }

    public func write(item entries: [PasteboardEntry]) -> Int {
        pb.clearContents()
        let item = NSPasteboardItem()
        for e in entries { item.setData(e.data, forType: NSPasteboard.PasteboardType(e.type)) }
        pb.writeObjects([item])
        return pb.changeCount
    }

    public func restore(_ snapshot: PasteboardSnapshot) {
        pb.clearContents()
        let items = snapshot.items.map { entries -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for e in entries { item.setData(e.data, forType: NSPasteboard.PasteboardType(e.type)) }
            return item
        }
        if !items.isEmpty { pb.writeObjects(items) }
    }
}
