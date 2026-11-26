// swift-tools-version: 6.2
// Testable Swift logic for the Mac app and widget, kept out of the SwiftUI views.
//
//   NacutKit      Foundation only: the widget snapshot and pure helpers. The widget links only this.
//   NacutKitCore  Logic that needs the core's UniFFI types (settings persistence, status mappings).
//
// Run the tests with `swift test --package-path apple/NacutKit` (after `apple/scripts/build-core.sh`).
import PackageDescription

let package = Package(
    name: "NacutKit",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "NacutKit", targets: ["NacutKit"]),
        .library(name: "NacutKitCore", targets: ["NacutKitCore"]),
    ],
    dependencies: [
        .package(path: "../NacutCore"),
    ],
    targets: [
        .target(name: "NacutKit"),
        .target(
            name: "NacutKitCore",
            dependencies: ["NacutKit", .product(name: "NacutCore", package: "NacutCore")]
        ),
        .testTarget(name: "NacutKitTests", dependencies: ["NacutKit"]),
        .testTarget(name: "NacutKitCoreTests", dependencies: ["NacutKitCore"]),
    ]
)
