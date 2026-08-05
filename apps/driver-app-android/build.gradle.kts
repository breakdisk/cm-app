plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.hilt) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.google.services) apply false
}

// Resolved here, in the root script scope, because the type-safe `libs` accessor
// is not available inside the `subprojects` lambda below.
val junit5Version = libs.versions.junit5.get()

/**
 * Route every module's unit tests through the JUnit Platform.
 *
 * The test sources are written against JUnit 5 (`org.junit.jupiter.api`), but
 * Gradle's default runner is the JUnit 4 platform. Without this, the `test` task
 * discovers zero Jupiter tests and reports success — so the suite looked green
 * while executing nothing. Only `feature:hub` had opted in individually; doing it
 * here covers every current and future module uniformly.
 *
 * Configured via the `Test` task type rather than the Android DSL so it stays
 * correct across AGP versions (AGP's unit-test tasks extend `Test`).
 */
subprojects {
    tasks.withType<Test>().configureEach {
        useJUnitPlatform()
    }
    plugins.withId("com.android.base") {
        // The engine is a runtime-only dependency, which a version-catalog bundle
        // cannot express — hence adding it here rather than in `testing-unit`.
        dependencies.add(
            "testRuntimeOnly",
            "org.junit.jupiter:junit-jupiter-engine:$junit5Version",
        )
        extensions.configure<com.android.build.gradle.BaseExtension>("android") {
            testOptions.unitTests.apply {
                // Unit tests compile against a stub android.jar whose methods throw
                // RuntimeException("Stub!"). android.util.Log is the one that bites:
                // any production code path logging on its way through a test blows up
                // the calling coroutine, surfacing as UncaughtExceptionsBeforeTest or
                // a silently skipped call rather than anything pointing at logging.
                // Returning defaults makes Log a no-op, which is what these tests want.
                isReturnDefaultValues = true
                // Surface failures in the Gradle console instead of only in the
                // HTML report, so CI logs are diagnosable without downloading it.
                all { it.testLogging { events("failed", "skipped") } }
            }
        }
    }
}
