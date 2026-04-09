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
    }

    buildTypes {
        debug {
            isDebuggable = true
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            buildConfigField("String", "BASE_URL", "\"https://api.logisticos.io/\"")
        }
    }

    flavorDimensions += "env"
    productFlavors {
        create("dev") {
            dimension = "env"
            applicationIdSuffix = ".dev"
            buildConfigField("String", "BASE_URL", "\"https://dev-api.logisticos.io/\"")
        }
        create("staging") {
            dimension = "env"
            buildConfigField("String", "BASE_URL", "\"https://staging-api.logisticos.io/\"")
        }
        create("prod") {
            dimension = "env"
            buildConfigField("String", "BASE_URL", "\"https://api.logisticos.io/\"")
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

    implementation(libs.material)
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
