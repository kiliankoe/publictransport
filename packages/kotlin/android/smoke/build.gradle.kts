// An app with the Android library that R8 has shrunk and renamed, as the
// release of a user's app would be. Its test runs on a device and shows what no test on the JVM can:
// that the Rust library is found and loaded, and that the rules of the
// library keep what R8 would otherwise remove.

plugins {
    alias(libs.plugins.android.application)
}

android {
    namespace = "io.github.kiliankoe.publictransport.smoke"
    compileSdk { version = release(37) { minorApiLevel = 0 } }
    buildToolsVersion = "37.0.0"
    ndkVersion = "30.0.16248370"

    defaultConfig {
        minSdk = 26
        targetSdk = 37
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    // R8 only renames and optimizes in a build that cannot be debugged.
    testBuildType = "release"
    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            testProguardFiles("test-rules.pro")
            // The app is installed on a test device only.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    // A recorded answer for the app to read.
    sourceSets["main"].assets.directories.add(rootProject.file("../../crates/core/tests/fixtures/efa").path)
}

dependencies {
    implementation(project(":android"))
    implementation(libs.mockwebserver)

    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.junit)
}
