plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.hilt)
    alias(libs.plugins.ksp)
}

android {
    namespace = "net.cargomarket.omnideliv.courier"
    compileSdk = 35

    defaultConfig {
        applicationId = "net.cargomarket.omnideliv.courier"
        // 26, not 30. WEBP_LOSSY would be cleaner at 30, but couriers in this
        // market are disproportionately on older hardware and raising the floor
        // to get a tidier constant trades real couriers for tidier code. The
        // encoder branches instead — see ProofEncoding.
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        debug {
            // Points at the deployed gateway rather than localhost: an emulator
            // cannot reach the host's loopback, and a default that silently
            // fails to connect looks like a broken app.
            buildConfigField("String", "API_BASE_URL", "\"https://os-api.cargomarket.net/\"")
            buildConfigField("String", "TENANT_SLUG", "\"demo\"")
            buildConfigField("String", "DEFAULT_COUNTRY_CODE", "\"63\"")
        }
        release {
            isMinifyEnabled = false
            buildConfigField("String", "API_BASE_URL", "\"https://os-api.cargomarket.net/\"")
            buildConfigField("String", "TENANT_SLUG", "\"cargomarket-ph\"")
            buildConfigField("String", "DEFAULT_COUNTRY_CODE", "\"63\"")
        }

        // The build a real courier can actually be handed today.
        //
        // `release` is the only variant that points at the live tenant, and
        // this app deliberately carries no signing material, so `assembleRelease`
        // produces an unsigned APK that no phone will install. `preview` is
        // `debug` — debug keystore, therefore installable — aimed at
        // cargomarket-ph. The slug is baked into the build, so an APK built for
        // `demo` searches `demo` forever and a courier registered from it lands
        // in the wrong tenant, which is indistinguishable from dispatch being
        // broken.
        create("preview") {
            initWith(getByName("debug"))
            matchingFallbacks += listOf("debug")
            buildConfigField("String", "API_BASE_URL", "\"https://os-api.cargomarket.net/\"")
            buildConfigField("String", "TENANT_SLUG", "\"cargomarket-ph\"")
            buildConfigField("String", "DEFAULT_COUNTRY_CODE", "\"63\"")
        }
    }

    // No signingConfigs block, deliberately. The sibling app has its release
    // keystore committed to this repository in three files under different
    // names; this one takes its signing material from CI secrets only, and
    // nothing signing-related belongs in the tree.

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.bundles.compose)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.material)
    // NotificationCompat, used directly by the shift service. It arrives
    // transitively via Compose today, but a direct API use on a transitive
    // dependency breaks the day that chain changes.
    implementation(libs.androidx.core.ktx)

    implementation(libs.hilt.android)
    implementation(libs.hilt.navigation.compose)
    implementation(libs.hilt.work)
    ksp(libs.hilt.compiler)
    ksp(libs.hilt.work.compiler)

    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)

    implementation(libs.retrofit.core)
    implementation(libs.retrofit.serialization)
    implementation(libs.okhttp.core)
    implementation(libs.okhttp.logging)
    implementation(libs.kotlinx.serialization.json)

    implementation(libs.coroutines.android)
    implementation(libs.coroutines.play.services)
    implementation(libs.workmanager.ktx)
    implementation(libs.security.crypto)
    implementation(libs.play.services.location)
    implementation(libs.bundles.camerax)

    testImplementation(libs.bundles.testing.unit)
    testImplementation(libs.okhttp.mockwebserver)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.room.testing)
    androidTestImplementation(libs.workmanager.test)
}
