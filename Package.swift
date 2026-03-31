// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "forepaw",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(name: "forepaw", targets: ["Forepaw"]),
        .library(name: "ForepawCore", targets: ["ForepawCore"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser.git", from: "1.5.0"),
    ],
    targets: [
        // CLI entry point
        .executableTarget(
            name: "Forepaw",
            dependencies: [
                "ForepawCore",
                "ForepawDarwin",
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
            ]
        ),
        // Platform-agnostic: protocol, ref system, tree rendering, output formatting
        .target(
            name: "ForepawCore"
        ),
        // macOS provider: AXUIElement, screencapture, CGEvent
        .target(
            name: "ForepawDarwin",
            dependencies: ["ForepawCore"]
        ),
        .testTarget(
            name: "ForepawCoreTests",
            dependencies: ["ForepawCore"]
        ),

    ]
)
