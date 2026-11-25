// swift-tools-version: 6.2
// The shared Rust core (core/nacut-ffi) for the Mac app: a prebuilt static library plus its
// UniFFI-generated Swift bindings. Build it with `apple/scripts/build-core.sh`.
import PackageDescription

let package = Package(
    name: "NacutCore",
    platforms: [.macOS(.v26)],
    products: [.library(name: "NacutCore", targets: ["NacutCore"])],
    targets: [
        .binaryTarget(name: "nacutFFI", path: "nacutFFI.xcframework"),
        .target(
            name: "NacutCore",
            dependencies: ["nacutFFI"],
            swiftSettings: [.swiftLanguageMode(.v5)],  // generated code predates Swift 6 checking
            linkerSettings: [
                .linkedFramework("Security"),
                .linkedFramework("CoreServices"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
    ]
)
