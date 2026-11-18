// swift-tools-version: 6.2
import PackageDescription

// The Swift package of the library. Its sources are in packages/swift; the
// manifest is here because SwiftPM only finds one at the root of a
// repository.
//
// The Rust library is a static library. On Apple platforms it comes in an
// XCFramework, which scripts/build-swift.sh builds. On Linux the same
// script builds the library, and the linker gets its directory with
// `-Xlinker -L<directory>`.
#if os(Linux)
let library: [Target] = []
let libraryDependency: [Target.Dependency] = []
let libraryLinking: [LinkerSetting] = [.linkedLibrary("publictransport_uniffi")]
#else
let library: [Target] = [
    .binaryTarget(
        name: "PublicTransportBinary",
        path: "packages/swift/PublicTransportBinary.xcframework"
    ),
]
let libraryDependency: [Target.Dependency] = ["PublicTransportBinary"]
let libraryLinking: [LinkerSetting] = []
#endif

let package = Package(
    name: "PublicTransport",
    platforms: [.iOS(.v26), .macOS(.v26), .tvOS(.v26), .watchOS(.v26), .visionOS(.v26)],
    products: [
        .library(name: "PublicTransport", targets: ["PublicTransport"]),
    ],
    targets: library + [
        // The C interface of the Rust library, which the generated Swift
        // code calls.
        .target(
            name: "publictransportFFI",
            dependencies: libraryDependency,
            path: "packages/swift/Sources/publictransportFFI",
            linkerSettings: libraryLinking
        ),
        .target(
            name: "PublicTransport",
            dependencies: ["publictransportFFI"],
            path: "packages/swift/Sources/PublicTransport"
        ),
        .testTarget(
            name: "PublicTransportTests",
            dependencies: ["PublicTransport"],
            path: "packages/swift/Tests/PublicTransportTests"
        ),
    ]
)
