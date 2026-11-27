/**
 * lumina-video Android Root Build Configuration
 *
 * This is the root build file for the lumina-video Android project.
 * Common configuration is defined here for all modules.
 */

plugins {
    id("com.android.application") version "9.0.1" apply false
    id("com.android.library") version "9.0.1" apply false
}

// Clean task to remove build directories
tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
