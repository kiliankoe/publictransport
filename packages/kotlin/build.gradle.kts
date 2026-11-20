import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.KotlinJvm
import com.vanniktech.maven.publish.SourcesJar
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.maven.publish)
    `java-library`
}

val repository = layout.projectDirectory.dir("../..")

// Every package has the version of the Rust workspace.
version =
    Regex("""(?m)^version = "(.+)"$""")
        .find(repository.file("Cargo.toml").asFile.readText())!!
        .groupValues[1]

dependencies {
    // A caller can pass an `OkHttpClient`, and needs coroutines to call a suspend function.
    api(libs.okhttp)
    api(libs.coroutines)
    implementation(libs.jna)

    testImplementation(kotlin("test"))
    testImplementation(libs.mockwebserver)
    // The version comes from the JUnit that kotlin-test brings.
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

kotlin {
    compilerOptions {
        allWarningsAsErrors = true
        jvmTarget = JvmTarget.JVM_11
        // Keeps the code from using what Java 11 lacks.
        freeCompilerArgs.add("-Xjdk-release=11")
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

// Cargo reads a relative directory from where it runs, the root of the repository.
val cargoTarget = providers.environmentVariable("CARGO_TARGET_DIR").orElse("target")
val library = cargoTarget.map { repository.asFile.resolve("$it/release/${System.mapLibraryName("publictransport_uniffi")}") }

// Cargo knows itself what is out of date, so this always runs.
val cargoBuild =
    tasks.register<Exec>("cargoBuild") {
        description = "Builds the Rust library for this machine."
        workingDir = repository.asFile
        commandLine("cargo", "build", "--quiet", "--release", "-p", "publictransport-uniffi")
    }

val generateBindings =
    tasks.register<Exec>("generateBindings") {
        description = "Generates the Kotlin code of the model from the Rust library."
        dependsOn(cargoBuild)
        val output = layout.buildDirectory.dir("uniffi")
        inputs.file(library)
        inputs.file(repository.file("crates/core/uniffi.toml"))
        outputs.dir(output)
        workingDir = repository.asFile
        val bindgen = "cargo run --quiet --release -p publictransport-uniffi --bin uniffi-bindgen --".split(" ")
        // Unformatted: the script below matches the code as UniFFI writes it.
        val arguments = listOf("--library", library.get().path, "--language", "kotlin", "--no-format")
        commandLine(bindgen + "generate" + arguments + listOf("--out-dir", output.get().asFile.path))
    }

val adaptBindings =
    tasks.register<Exec>("adaptBindings") {
        description = "Makes the generated code compile and read like Kotlin, see the script."
        val script = repository.file("scripts/kotlin-idioms.py")
        val generated =
            generateBindings.map {
                it.outputs.files.singleFile
                    .resolve("publictransport/publictransport.kt")
            }
        val output = layout.buildDirectory.file("generated/kotlin/publictransport.kt")
        inputs.file(script)
        inputs.file(generated)
        outputs.dir(output.map { it.asFile.parentFile })
        commandLine(script.asFile.path, generated.get().path, output.get().asFile.path)
    }

// The directory that JNA searches on the class path for this machine's library.
val jnaPlatform =
    run {
        val system = System.getProperty("os.name").lowercase()
        val processor = System.getProperty("os.arch")
        val os =
            when {
                system.startsWith("mac") -> "darwin"
                system.startsWith("windows") -> "win32"
                else -> "linux"
            }
        "$os-${if (processor in setOf("amd64", "x86_64")) "x86-64" else processor}"
    }

val nativeLibrary =
    tasks.register<Copy>("nativeLibrary") {
        description = "Puts the Rust library where JNA finds it on the class path."
        dependsOn(cargoBuild)
        from(library)
        into(layout.buildDirectory.dir("native"))
        eachFile { path = "$jnaPlatform/$name" }
    }

// A release passes a directory with the libraries of every platform, each
// in the directory that JNA searches, see the release workflow.
val nativeLibraries = providers.gradleProperty("nativeLibraries")

sourceSets.main {
    kotlin.srcDir(adaptBindings)
}

// Not a source directory, which would put the libraries into the sources jar.
tasks.processResources {
    from(nativeLibraries.orNull ?: nativeLibrary)
}

tasks.test {
    useJUnitPlatform()
    // The answers that the tests replay.
    inputs.dir(repository.dir("crates/core/tests/fixtures"))
    // JNA loads the Rust library, which newer JVMs warn about otherwise.
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

// Signing is switched on by the release workflow, so that publishing to the
// local Maven repository needs no key.
mavenPublishing {
    publishToMavenCentral()
    coordinates(group.toString(), "publictransport", version.toString())
    // The documentation is in the sources. Maven Central asks for the file.
    configure(KotlinJvm(javadocJar = JavadocJar.Empty(), sourcesJar = SourcesJar.Sources()))
    pom {
        name = "publictransport"
        description = "A client for public transport APIs with one model for stops, departures and trips"
        url = "https://kiliankoe.github.io/publictransport/"
        licenses {
            license {
                name = "MIT"
                url = "https://opensource.org/license/mit"
            }
            license {
                name = "Apache-2.0"
                url = "https://www.apache.org/licenses/LICENSE-2.0"
            }
        }
        developers {
            developer {
                id = "kiliankoe"
                name = "Kilian Koeltzsch"
                url = "https://github.com/kiliankoe"
            }
        }
        scm {
            url = "https://github.com/kiliankoe/publictransport"
            connection = "scm:git:https://github.com/kiliankoe/publictransport.git"
            developerConnection = "scm:git:ssh://git@github.com/kiliankoe/publictransport.git"
        }
    }
}
