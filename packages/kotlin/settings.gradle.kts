rootProject.name = "publictransport"

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}

// The example programs, built against this checkout of the package.
include("examples")
project(":examples").projectDir = file("../../examples/kotlin")
