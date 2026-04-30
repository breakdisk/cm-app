import java.util.Base64
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.hilt)
    alias(libs.plugins.ksp)
    alias(libs.plugins.google.services)
}

val localProps = Properties().apply {
    val f = rootProject.file("local.properties")
    if (f.exists()) load(f.inputStream())
}

android {
    namespace = "io.logisticos.driver"
    compileSdk = 35

    defaultConfig {
        applicationId = "cargomarket.driver"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"
        testInstrumentationRunner = "io.logisticos.driver.HiltTestRunner"
        val mapsApiKey = localProps.getProperty("GOOGLE_MAPS_API_KEY") ?: ""
        manifestPlaceholders["MAPS_API_KEY"] = mapsApiKey
        buildConfigField("String", "MAPS_API_KEY", "\"$mapsApiKey\"")
        val mapboxAccessToken = localProps.getProperty("MAPBOX_ACCESS_TOKEN") ?: ""
        buildConfigField("String", "MAPBOX_ACCESS_TOKEN", "\"$mapboxAccessToken\"")
    }

    signingConfigs {
        // Only materialise the release signing config when KEYSTORE_BASE64 is present
        // (i.e. in CI). Without this guard, the error() calls fire during every
        // Gradle configuration phase — including assembleDebug on developer machines.
        val keystoreB64 = System.getenv("KEYSTORE_BASE64")
        if (keystoreB64 != null) {
            create("release") {
                val keystoreFile = rootProject.file("keystore.jks")
                keystoreFile.writeBytes(Base64.getMimeDecoder().decode(keystoreB64))
                storeFile = keystoreFile
                storePassword = System.getenv("KEYSTORE_PASSWORD")
                    ?: error("KEYSTORE_PASSWORD env var is required for release builds")
                keyAlias = System.getenv("KEY_ALIAS")
                    ?: error("KEY_ALIAS env var is required for release builds")
                keyPassword = System.getenv("KEY_PASSWORD")
                    ?: error("KEY_PASSWORD env var is required for release builds")
            }
        }
    }

    buildTypes {
        debug {
            isDebuggable = true
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Null when KEYSTORE_BASE64 is absent (local debug runs). CI always provides it,
            // so a CI release build without the secret will error inside signingConfigs above.
            signingConfig = signingConfigs.findByName("release")
        }
    }

    flavorDimensions += "env"
    productFlavors {
        create("dev") {
            dimension = "env"
            applicationIdSuffix = ".dev"
            val devUrl = localProps.getProperty("API_BASE_URL") ?: "https://os-api.cargomarket.net/"
            val tenantId = localProps.getProperty("TENANT_ID") ?: "demo"
            buildConfigField("String", "BASE_URL", "\"$devUrl\"")
            buildConfigField("String", "TENANT_ID", "\"$tenantId\"")
        }
        create("staging") {
            dimension = "env"
            buildConfigField("String", "BASE_URL", "\"https://staging-api.cargomarket.net/\"")
            buildConfigField("String", "TENANT_ID", "\"atlas-cargo-ae\"")
        }
        create("prod") {
            dimension = "env"
            buildConfigField("String", "BASE_URL", "\"https://os-api.cargomarket.net/\"")
            buildConfigField("String", "TENANT_ID", "\"atlas-cargo-ae\"")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        jniLibs {
            excludes += listOf(
                "**/libandroid-tests-support-code.so",
                "**/libtoolChecker.so",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(project(":core:common"))
    implementation(project(":core:network"))
    implementation(project(":core:database"))
    implementation(project(":core:location"))
    implementation(project(":feature:auth"))
    implementation(project(":feature:home"))
    implementation(project(":feature:route"))
    implementation(project(":feature:navigation"))
    implementation(project(":feature:delivery"))
    implementation(project(":feature:pod"))
    implementation(project(":feature:scanner"))
    implementation(project(":feature:pickup"))
    implementation(project(":feature:notifications"))
    implementation(project(":feature:profile"))
    implementation(project(":feature:assignment"))

    implementation(libs.material)
    implementation(libs.mapbox.maps)
    implementation(libs.okhttp.logging)
    implementation(platform(libs.compose.bom))
    implementation(libs.bundles.compose)
    implementation(libs.hilt.android)
    implementation(libs.hilt.navigation.compose)
    implementation(libs.hilt.work)
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    implementation(libs.rootbeer)
    ksp(libs.hilt.compiler)
    ksp(libs.hilt.work.compiler)

    testImplementation(libs.bundles.testing.unit)
    testImplementation(libs.junit5.engine)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.hilt.testing)
    androidTestImplementation(libs.compose.ui.test)
    debugImplementation(platform(libs.compose.bom))
    debugImplementation(libs.compose.ui.test.manifest)
    kspAndroidTest(libs.hilt.compiler)
}
