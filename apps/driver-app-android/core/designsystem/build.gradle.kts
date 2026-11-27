plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// The driver design system (mobile design handoff): colour roles for night and
// sun mode, the condensed heading face, and the glove-sized pieces every screen
// is built from. No Mapbox, no network — so it and the screens using it compile
// without the Mapbox download token the :app module needs.
android {
    namespace = "io.logisticos.driver.core.designsystem"
    compileSdk = 35
    defaultConfig { minSdk = 26 }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.bundles.compose)
    testImplementation(libs.bundles.testing.unit)
}
