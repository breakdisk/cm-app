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
    // The engine is a runtime-only dependency, which a version-catalog bundle
    // cannot express — hence adding it here rather than in `testing-unit`.
    plugins.withId("com.android.base") {
        dependencies.add(
            "testRuntimeOnly",
            "org.junit.jupiter:junit-jupiter-engine:$junit5Version",
        )
    }
}
