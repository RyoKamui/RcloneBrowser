// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "RcloneBrowserNative",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "RcloneBrowserNative", targets: ["RcloneBrowserNative"])
    ],
    targets: [
        .systemLibrary(
            name: "CRcloneCore",
            path: "Sources/CRcloneCore"
        ),
        .executableTarget(
            name: "RcloneBrowserNative",
            dependencies: ["CRcloneCore"],
            path: "Sources/RcloneBrowserNative",
            linkerSettings: [
                .unsafeFlags(["-L", "rust-core/target/release"]),
                .linkedLibrary("rclone_browser_core"),
                .linkedFramework("AppKit")
            ]
        ),
        .testTarget(
            name: "RcloneBrowserNativeTests",
            dependencies: ["RcloneBrowserNative"],
            path: "Tests/RcloneBrowserNativeTests"
        )
    ],
    swiftLanguageModes: [.v5]
)
