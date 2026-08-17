pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "OmniDelivCourier"

// One module, deliberately.
//
// The sibling driver app is split into ten (`core:*`, `feature:*`), and that is
// the right shape for it. This app cannot be built on the development machine —
// there is no local Gradle — so every line lands unverified until CI runs, and
// inter-module wiring is the part of a Gradle build most likely to fail in ways
// that are opaque from the log alone. Packages give the same separation at none
// of that risk. Split it once there is a green build to split from.
include(":app")
