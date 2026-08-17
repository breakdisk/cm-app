plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.hilt) apply false
    alias(libs.plugins.ksp) apply false
}

// Resolved in the root script scope: the type-safe `libs` accessor is not
// available inside the `subprojects` lambda below.
val junit5Version = libs.versions.junit5.get()

subprojects {
    tasks.withType<Test>().configureEach {
        // The test sources are JUnit 5. Gradle's default runner is the JUnit 4
        // platform, which discovers zero Jupiter tests and reports success — a
        // suite that looks green while executing nothing. The sibling app was
        // bitten by exactly this.
        useJUnitPlatform()

        maxHeapSize = "1g"

        // A test worker that exhausts its heap does not reliably die: the OOM
        // surfaces on whichever thread is allocating, and if that is the
        // worker's connection thread the process lives on but can never report
        // its result. Gradle then waits for a message that never arrives and
        // the build hangs rather than failing — on CI that burns the full
        // six-hour job limit. Exiting on the first OOM turns it into a prompt
        // failure.
        jvmArgs("-XX:+ExitOnOutOfMemoryError")
    }

    plugins.withId("com.android.base") {
        // Runtime-only, which a version-catalog bundle cannot express.
        dependencies.add(
            "testRuntimeOnly",
            "org.junit.jupiter:junit-jupiter-engine:$junit5Version",
        )
        extensions.configure<com.android.build.gradle.BaseExtension>("android") {
            testOptions.unitTests.apply {
                // Unit tests compile against a stub android.jar whose methods
                // throw RuntimeException("Stub!"). android.util.Log is the one
                // that bites: any production path logging on its way through a
                // test blows up the calling coroutine, and it surfaces as
                // something that points nowhere near logging.
                isReturnDefaultValues = true
                all { it.testLogging { events("failed", "skipped") } }
            }
        }
    }
}
