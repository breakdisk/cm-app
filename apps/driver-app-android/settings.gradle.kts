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
        maven {
            url = uri("https://api.mapbox.com/downloads/v2/releases/maven")
            authentication {
                create<BasicAuthentication>("basic")
            }
            credentials {
                username = "mapbox"
                password = (settings.extra.properties["MAPBOX_DOWNLOADS_TOKEN"]
                    ?: System.getenv("MAPBOX_DOWNLOADS_TOKEN")
                    ?: "") as String
            }
        }
    }
}

rootProject.name = "DriverApp"

include(":app")
include(":core:network")
include(":core:database")
include(":core:location")
include(":core:common")
include(":feature:auth")
include(":feature:home")
include(":feature:route")
include(":feature:navigation")
include(":feature:delivery")
include(":feature:pod")
include(":feature:scanner")
include(":feature:pickup")
include(":feature:notifications")
include(":feature:profile")
