// swift-tools-version:5.10
// KeepsayKit: shared Swift code for the macOS menu bar app (M2) and the iOS
// app (M4a). Apple frameworks only; no third-party packages (PROPOSAL §9).
//
// The Rust core is linked only when `apple/scripts/build-core.sh` has
// produced both the XCFramework and the generated UniFFI Swift bindings.
// Without them the package still builds (and its tests run) against
// `UnavailableCore`, so the platform code can be developed and tested before
// the FFI exists. The iOS keyboard extension (M4b) must never depend on this
// package's `KeepsayCoreBindings` target (INV-10).

import Foundation
import PackageDescription

let packageDir = Context.packageDirectory
let xcframeworkPath = "Frameworks/KeepsayCoreFFI.xcframework"
let bindingsDir = "Sources/KeepsayCoreBindings"
let hasCore =
    FileManager.default.fileExists(atPath: "\(packageDir)/\(xcframeworkPath)")
    && FileManager.default.fileExists(atPath: "\(packageDir)/\(bindingsDir)/keepsay_core.swift")

var kitDependencies: [Target.Dependency] = []
var targets: [Target] = []

if hasCore {
    kitDependencies.append("KeepsayCoreBindings")
    targets += [
        .binaryTarget(name: "KeepsayCoreFFI", path: xcframeworkPath),
        .target(
            name: "KeepsayCoreBindings",
            dependencies: ["KeepsayCoreFFI"],
            path: bindingsDir,
            linkerSettings: [
                // rustls-platform-verifier and the Keychain vault.
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
                // whisper.cpp (C++ runtime, BLAS).
                .linkedFramework("Accelerate"),
                .linkedLibrary("c++"),
            ]
        ),
    ]
}

targets += [
    .target(name: "KeepsayKit", dependencies: kitDependencies, path: "Sources/KeepsayKit"),
    .testTarget(name: "KeepsayKitTests", dependencies: ["KeepsayKit"], path: "Tests/KeepsayKitTests"),
]

let package = Package(
    name: "KeepsayKit",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "KeepsayKit", targets: ["KeepsayKit"]),
    ],
    targets: targets
)
