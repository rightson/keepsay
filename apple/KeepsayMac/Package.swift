// swift-tools-version:5.10
// KeepsayMac: the macOS menu bar app (M2). Built with SwiftPM and wrapped
// into Keepsay.app by apple/scripts/build-mac-app.sh (docs/changes/0015).
// Apple frameworks only (PROPOSAL §9).

import PackageDescription

let package = Package(
    name: "KeepsayMac",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "KeepsayMac", targets: ["KeepsayMac"]),
    ],
    dependencies: [
        .package(path: "../KeepsayKit"),
    ],
    targets: [
        // System integration with unit-testable rules: the fn event tap
        // (single isolated file, THREAT_MODEL T7), Carbon hotkeys, text
        // insertion, pasteboard backup/restore, input source switching.
        .target(
            name: "KeepsayMacSupport",
            dependencies: [.product(name: "KeepsayKit", package: "KeepsayKit")],
            path: "Sources/KeepsayMacSupport"
        ),
        .executableTarget(
            name: "KeepsayMac",
            dependencies: [
                "KeepsayMacSupport",
                .product(name: "KeepsayKit", package: "KeepsayKit"),
            ],
            path: "Sources/KeepsayMac"
        ),
        .testTarget(
            name: "KeepsayMacSupportTests",
            dependencies: ["KeepsayMacSupport", .product(name: "KeepsayKit", package: "KeepsayKit")],
            path: "Tests/KeepsayMacSupportTests"
        ),
    ]
)
