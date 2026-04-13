plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.hilt)
    alias(libs.plugins.ksp)
}

android {
    namespace = "io.logisticos.driver.core.location"
    compileSdk = 35
    defaultConfig { minSdk = 26 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(project(":core:database"))
    implementation(project(":core:common"))
    implementation(libs.play.services.location)
    implementation(libs.hilt.android)
    implementation(libs.workmanager.ktx)
    implementation(libs.hilt.work)
    implementation(libs.coroutines.android)
    implementation(libs.coroutines.play.services)
    ksp(libs.hilt.compiler)
    ksp(libs.hilt.work.compiler)
    testImplementation(libs.bundles.testing.unit)
    testImplementation(libs.mockk.android)
}
