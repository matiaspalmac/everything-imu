import org.gradle.api.tasks.Exec
import java.io.File

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

    testOptions {
        // SlimeVrClient logs via android.util.Log, which is a stub in JVM unit
        // tests. Return defaults instead of throwing "not mocked".
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    api(libs.androidx.core.ktx)
    api(libs.androidx.lifecycle.service)
    api(libs.androidx.datastore.preferences)
    api(libs.kotlinx.coroutines.android)
    api(libs.play.services.wearable)
    api(libs.kotlinx.coroutines.play.services)
    api(libs.androidx.fragment)

    testImplementation(libs.junit)
}

// cargo-ndk integration. Cross-compiles `crates/jni-android` to native .so files
// under `core/src/main/jniLibs/<abi>/`. Every gating decision is made at
// configuration time and reduced to plain values so the task closure carries
// no Project / script references — required for Gradle's configuration cache.

val skipCargoNdk: Boolean =
    (project.findProperty("skipCargoNdk") as? String)?.toBoolean() ?: false

val workspaceRootPath: String = rootProject.projectDir.parentFile.absolutePath
val jniLibsAbsPath: String = project.file("src/main/jniLibs").absolutePath

val ndkHome: String? = run {
    System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("NDK_HOME")
        ?: run {
            val localProps = rootProject.file("local.properties")
            if (localProps.exists()) {
                localProps.useLines { lines ->
                    for (line in lines) {
                        if (line.startsWith("ndk.dir=")) {
                            return@run line.substringAfter("=").trim()
                        }
                    }
                }
            }
            val sdkRoot = System.getenv("ANDROID_SDK_ROOT")
                ?: System.getenv("ANDROID_HOME")
                ?: (System.getProperty("user.home") + "/AppData/Local/Android/Sdk")
            val ndkRoot = File(sdkRoot, "ndk")
            if (!ndkRoot.isDirectory) {
                null
            } else {
                ndkRoot.listFiles { f -> f.isDirectory }
                    ?.maxByOrNull { it.name }
                    ?.absolutePath
            }
        }
}

fun executableOnPath(executable: String): Boolean {
    val pathDirs = System.getenv("PATH")?.split(File.pathSeparator).orEmpty()
    val candidates = if (System.getProperty("os.name").lowercase().contains("win")) {
        listOf("$executable.exe", "$executable.cmd", "$executable.bat")
    } else {
        listOf(executable)
    }
    return pathDirs.any { dir -> candidates.any { File(dir, it).canExecute() } }
}

val cargoOnPath: Boolean = executableOnPath("cargo")
val cargoNdkOnPath: Boolean = executableOnPath("cargo-ndk")
val shouldRunCargoNdk: Boolean = !skipCargoNdk && cargoOnPath && cargoNdkOnPath

val buildJniAndroid = tasks.register<Exec>("buildJniAndroid") {
    group = "build"
    description = "Cross-compile crates/jni-android via cargo-ndk to core/src/main/jniLibs."

    workingDir = File(workspaceRootPath)
    isIgnoreExitValue = true

    if (ndkHome != null) {
        environment("ANDROID_NDK_HOME", ndkHome)
    }

    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "armeabi-v7a",
        "-t", "x86_64",
        "-o", jniLibsAbsPath,
        "build", "-p", "jni-android", "--release",
    )

    // Capture only primitives — no Project / logger / script-function refs.
    val enabled = shouldRunCargoNdk
    val skipReason = when {
        skipCargoNdk -> "skipCargoNdk=true"
        !cargoOnPath -> "'cargo' not found on PATH — using prebuilt jniLibs/"
        !cargoNdkOnPath -> "'cargo-ndk' not installed; run `cargo install cargo-ndk`"
        else -> null
    }
    onlyIf("cargo-ndk available and not skipped") {
        if (!enabled) {
            it.logger.lifecycle("buildJniAndroid: skipped ({})", skipReason ?: "disabled")
        }
        enabled
    }
}

tasks.named("preBuild").configure { dependsOn(buildJniAndroid) }
