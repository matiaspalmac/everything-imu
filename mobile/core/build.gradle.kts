plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "cl.matiaspalma.everythingimu.core"
    compileSdk = 35

    defaultConfig {
        minSdk = 21
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        jvmToolchain(17)
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

dependencies {
    api(libs.androidx.core.ktx)
    api(libs.androidx.lifecycle.service)
    api(libs.androidx.datastore.preferences)
    api(libs.kotlinx.coroutines.android)
}

/**
 * cargo-ndk integration. Cross-compiles `crates/jni-android` to native .so files
 * under `core/src/main/jniLibs/<abi>/`. Skipped automatically when:
 *   - `cargo` or `cargo-ndk` is not on PATH
 *   - `ANDROID_NDK_HOME` is not set and no NDK is auto-discovered
 *   - the project property `skipCargoNdk=true` is passed (CI / quick iterations)
 *
 * Disable explicitly with:
 *   ./gradlew :app-mobile:assembleDebug -PskipCargoNdk=true
 */
val skipCargoNdk: Boolean = (project.findProperty("skipCargoNdk") as? String)?.toBoolean() ?: false

val workspaceRoot: File = rootProject.projectDir.parentFile

val buildJniAndroid = tasks.register<Exec>("buildJniAndroid") {
    group = "build"
    description = "Cross-compile crates/jni-android via cargo-ndk to core/src/main/jniLibs."

    val jniLibsDir = project.file("src/main/jniLibs")
    workingDir = workspaceRoot
    isIgnoreExitValue = true

    val ndkHome = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("NDK_HOME")
        ?: defaultNdkPath()

    if (ndkHome != null) {
        environment("ANDROID_NDK_HOME", ndkHome)
    }

    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "armeabi-v7a",
        "-t", "x86_64",
        "-o", jniLibsDir.absolutePath,
        "build", "-p", "jni-android", "--release",
    )

    onlyIf {
        if (skipCargoNdk) {
            logger.lifecycle("buildJniAndroid: skipped (skipCargoNdk=true)")
            return@onlyIf false
        }
        if (!isOnPath("cargo")) {
            logger.warn("buildJniAndroid: 'cargo' not found on PATH — using prebuilt jniLibs/")
            return@onlyIf false
        }
        if (!isOnPath("cargo-ndk")) {
            logger.warn("buildJniAndroid: 'cargo-ndk' not installed; run `cargo install cargo-ndk`")
            return@onlyIf false
        }
        true
    }
}

tasks.named("preBuild").configure { dependsOn(buildJniAndroid) }

fun defaultNdkPath(): String? {
    val localProps = rootProject.file("local.properties")
    if (localProps.exists()) {
        localProps.useLines { lines ->
            for (line in lines) {
                if (line.startsWith("ndk.dir=")) return line.substringAfter("=").trim()
            }
        }
    }
    val sdkRoot = System.getenv("ANDROID_SDK_ROOT")
        ?: System.getenv("ANDROID_HOME")
        ?: (System.getProperty("user.home") + "/AppData/Local/Android/Sdk")
    val ndkRoot = File(sdkRoot, "ndk")
    if (!ndkRoot.isDirectory) return null
    return ndkRoot.listFiles { f -> f.isDirectory }
        ?.maxByOrNull { it.name }
        ?.absolutePath
}

fun isOnPath(executable: String): Boolean {
    val pathDirs = System.getenv("PATH")?.split(File.pathSeparator).orEmpty()
    val candidates = if (System.getProperty("os.name").lowercase().contains("win")) {
        listOf("$executable.exe", "$executable.cmd", "$executable.bat")
    } else {
        listOf(executable)
    }
    return pathDirs.any { dir -> candidates.any { File(dir, it).canExecute() } }
}
