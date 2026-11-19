plugins {
    alias(libs.plugins.kotlin.jvm)
}

dependencies {
    implementation(project(":"))
}

// One task per program: `gradle :examples:trips`.
for (name in listOf("locations", "departures", "trips", "networks")) {
    tasks.register<JavaExec>(name) {
        group = "examples"
        description = "Runs the example ${name.replaceFirstChar(Char::uppercase)}.kt."
        classpath = sourceSets.main.get().runtimeClasspath
        mainClass = "${name.replaceFirstChar(Char::uppercase)}Kt"
        // JNA loads the Rust library, which newer JVMs warn about otherwise.
        jvmArgs("--enable-native-access=ALL-UNNAMED")
    }
}
