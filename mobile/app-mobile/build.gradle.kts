import java.io.FileInputStream
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// Release signing. Reads keystore.properties at the mobile root locally; falls
// back to env vars in CI. When neither is present the release build stays
// unsigned so contributors and CI without secrets can still assemble it.
val keystoreProps = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) FileInputStream(f).use { load(it) }
}
fun signingProp(key: String, env: String): String? =
    keystoreProps.getProperty(key) ?: System.getenv(env)
val releaseStoreFile = signingProp("storeFile", "ANDROID_RELEASE_KEYSTORE")
val releaseStorePassword = signingProp("storePassword", "ANDROID_RELEASE_KEYSTORE_PASSWORD")
val releaseKeyAlias = signingProp("keyAlias", "ANDROID_RELEASE_KEY_ALIAS")
val releaseKeyPassword = signingProp("keyPassword", "ANDROID_RELEASE_KEY_PASSWORD")
val hasReleaseSigning = releaseStoreFile != null && releaseStorePassword != null &&
    releaseKeyAlias != null && releaseKeyPassword != null

android {
    namespace = "cl.matiaspalma.everythingimu.mobile"
    compileSdk = 35

    defaultConfig {
        applicationId = "cl.matiaspalma.everythingimu.mobile"
        minSdk = 21
        targetSdk = 35
        versionCode = 5
        versionName = "1.0.4"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        jvmToolchain(17)
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = rootProject.file(releaseStoreFile!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (hasReleaseSigning) signingConfig = signingConfigs.getByName("release")
        }
    }
}

dependencies {
    implementation(project(":core"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.activity.compose)

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
}
