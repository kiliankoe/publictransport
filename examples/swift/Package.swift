// swift-tools-version: 6.2
import PackageDescription

// The examples of the Swift package, one program each.
//
// They use the package of this checkout, see "Development" in
// packages/swift/README.md for the Rust library that needs. An application
// depends on a release by the URL of the repository instead.
let library: Target.Dependency = .product(name: "PublicTransport", package: "publictransport")

let package = Package(
    name: "PublicTransportExamples",
    platforms: [.macOS(.v26)],
    dependencies: [.package(name: "publictransport", path: "../..")],
    targets: [
        .executableTarget(name: "locations", dependencies: [library]),
        .executableTarget(name: "departures", dependencies: [library]),
        .executableTarget(name: "trips", dependencies: [library]),
        .executableTarget(name: "networks", dependencies: [library]),
    ]
)
