// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "UsageCore",
    platforms: [.macOS(.v26)],
    products: [.library(name: "UsageCore", targets: ["UsageCore"])],
    targets: [
        .target(
            name: "UsageCore",
            resources: [.process("Pricing/Pricing.json")]
        ),
        .testTarget(
            name: "UsageCoreTests",
            dependencies: ["UsageCore"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
