// swift-tools-version: 6.2
import PackageDescription

// The Swift package of the library. Its sources are in packages/swift; the
// manifest is here because SwiftPM only finds one at the root of a
// repository.
//
// The Rust library is a static library. On Apple platforms it comes in an
// XCFramework: the one of the last release, or with
// PUBLICTRANSPORT_LOCAL_BINARY=1 in the environment the one that
// scripts/build-swift.sh built into the checkout. On Linux the linker gets
// the directory of the library with `-Xlinker -L<directory>`.
//
// The release workflow writes these two lines. Between releases the code
// can be ahead of that binary, so depend on a version, not on a branch.
let binaryURL =
    "https://github.com/kiliankoe/publictransport/releases/download/v0.1.2/PublicTransportBinary.xcframework.zip"
let binaryChecksum = "d47058cf6d5171e4bf3933d3342bbf608cf9417fa74e6310442bf582a0ce3fdb"

#if os(Linux)
let library: [Target] = []
let libraryDependency: [Target.Dependency] = []
let libraryLinking: [LinkerSetting] = [.linkedLibrary("publictransport_uniffi")]
#else
// The environment and not a look at the directory: SwiftPM keeps the
// manifest it evaluated and would not see the directory appear.
let library: [Target] = [
    Context.environment["PUBLICTRANSPORT_LOCAL_BINARY"] != nil
        ? .binaryTarget(name: "PublicTransportBinary", path: "packages/swift/PublicTransportBinary.xcframework")
        : .binaryTarget(name: "PublicTransportBinary", url: binaryURL, checksum: binaryChecksum),
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
