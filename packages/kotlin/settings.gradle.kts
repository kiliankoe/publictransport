rootProject.name = "publictransport"

pluginManagement {
    repositories {
        gradlePluginPortal()
        // The Android Gradle Plugin
        google()
    }
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
        google()
    }
}

// The example programs, built against this checkout of the package.
include("examples")
project(":examples").projectDir = file("../../examples/kotlin")

// The Android library needs the Android SDK and the Rust targets for
// Android. The Android shells of the flake set this property.
if (providers.gradleProperty("android").isPresent) {
    include("android")
    // An app that R8 has shrunk, for the test of the library on a device.
    include("android:smoke")
}
