import com.vanniktech.maven.publish.AndroidSingleVariantLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SourcesJar
import org.gradle.api.publish.maven.MavenPom
import javax.inject.Inject

// The Kotlin package as an Android library: the same code, with the Rust
// library built for the Android ABIs instead of the desktop platforms.

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.maven.publish)
}

group = rootProject.group
version = rootProject.version

android {
    namespace = "io.github.kiliankoe.publictransport"
    // These three are the versions of the `android` shell in flake.nix.
    compileSdk { version = release(37) { minorApiLevel = 0 } }
    buildToolsVersion = "37.0.0"
    ndkVersion = "30.0.16248370"

    defaultConfig {
        // The model holds times as `java.time` types, which Android has from here on.
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    sourceSets["main"].kotlin.directories.add(rootProject.file("src/main/kotlin").path)
}

kotlin {
    compilerOptions {
        allWarningsAsErrors = true
        // The name of the Gradle project would be `android`, which is in the
        // name of a file in the library that has to be unique in an app.
        moduleName = "publictransport-android"
    }
}

dependencies {
    api(libs.okhttp)
    api(libs.coroutines)
    // The jar has the libraries of JNA for desktop platforms only.
    implementation(libs.jna) { artifact { type = "aar" } }
}

/** Builds the Rust library for the Android ABIs, each into the directory of its ABI. */
abstract class CargoNdk : DefaultTask() {
    @get:Internal
    abstract val repository: DirectoryProperty

    @get:Input
    abstract val minSdk: Property<Int>

    @get:OutputDirectory
    abstract val libraries: DirectoryProperty

    @get:Inject
    abstract val exec: ExecOperations

    @TaskAction
    fun build() {
        val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64").flatMap { listOf("-t", it) }
        val output = listOf("-P", minSdk.get().toString(), "-o", libraries.get().asFile.path)
        exec.exec {
            workingDir = repository.get().asFile
            commandLine(listOf("cargo", "ndk") + abis + output + "build --quiet --profile dist -p publictransport-uniffi".split(" "))
        }
    }
}

val cargoNdk =
    tasks.register<CargoNdk>("cargoNdk") {
        description = "Builds the Rust library for the Android ABIs."
        repository = rootProject.layout.projectDirectory.dir("../..")
        minSdk = android.defaultConfig.minSdk
        // Cargo knows itself what is out of date, so this always runs.
        outputs.upToDateWhen { false }
    }

/** Hands the generated code of the model to the Android build as a source directory of its own. */
abstract class Bindings : DefaultTask() {
    @get:InputDirectory
    abstract val generated: DirectoryProperty

    @get:OutputDirectory
    abstract val sources: DirectoryProperty

    @get:Inject
    abstract val files: FileSystemOperations

    @TaskAction
    fun copy() {
        files.sync {
            from(generated)
            into(sources)
        }
    }
}

val bindings =
    tasks.register<Bindings>("bindings") {
        description = "Takes the generated code of the model from the JVM package."
        // The JVM package generates it from the Rust library of this machine.
        dependsOn(rootProject.tasks.named("adaptBindings"))
        generated = rootProject.layout.buildDirectory.dir("generated/kotlin")
    }

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdk::libraries)
        variant.sources.kotlin?.addGeneratedSourceDirectory(bindings, Bindings::sources)
    }
}

mavenPublishing {
    publishToMavenCentral()
    coordinates(group.toString(), "publictransport-android", version.toString())
    configure(AndroidSingleVariantLibrary(javadocJar = JavadocJar.Empty(), sourcesJar = SourcesJar.Sources(), variant = "release"))
    @Suppress("UNCHECKED_CAST")
    pom(rootProject.extra["pom"] as Action<MavenPom>)
    pom { name = "publictransport-android" }
}
