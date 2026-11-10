plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "fyi.omochi.mobile"
    compileSdk = flutter.compileSdkVersion
    // iroh_flutter requires NDK r29+. Keep native builds reproducible.
    ndkVersion = "30.0.16248370"

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID
        // (https://developer.android.com/studio/build/application-id.html).
        applicationId = "fyi.omochi.mobile"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = maxOf(24, flutter.minSdkVersion)
        ndk { abiFilters.addAll(listOf("arm64-v8a", "x86_64")) }
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter.
        // (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P
        // force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// Cargo owns native sources; generated binaries stay under the build directory.
val sharedHubJni = layout.buildDirectory.dir("shared-hub/jniLibs")

abstract class BuildSharedHub : Exec() {
    @get:OutputDirectory abstract val jniLibs: DirectoryProperty
}

val buildSharedHub by
    tasks.registering(BuildSharedHub::class) {
        jniLibs.set(sharedHubJni)
        // Cargo tracks source/dependency changes, including the separate repository.
        outputs.upToDateWhen { false }
        val appRoot = rootProject.projectDir.parentFile
        val requested =
            providers.gradleProperty("target-platform").orElse("android-arm64,android-x64").get()
        val abis =
            requested.split(",").mapNotNull { platform ->
                when (platform) {
                    "android-arm64" -> "arm64-v8a"
                    "android-x64" -> "x86_64"
                    "android-arm" -> null
                    else ->
                        error(
                            "Shared Hub supports Android arm64 and x86_64; select --target-platform android-arm64,android-x64",
                        )
                }
            }
        require(abis.isNotEmpty()) { "Shared Hub requires Android arm64 or x86_64" }
        val release =
            gradle.startParameter.taskNames.any {
                it.contains("Release", ignoreCase = true) ||
                    it.contains("Profile", ignoreCase = true)
            }
        commandLine(
            listOf(
                "python3",
                "${appRoot}/tool/build_shared_hub.py",
                "--platform",
                "android",
                "--profile",
                if (release) "release" else "debug",
                "--output",
                sharedHubJni.get().asFile.path,
            ) + abis.flatMap { listOf("--abi", it) },
        )
    }

androidComponents.onVariants { variant ->
    variant.sources.jniLibs?.addGeneratedSourceDirectory(buildSharedHub) { it.jniLibs }
}

// Same helper ABI as rustls-platform-verifier-android in the Rust lockfile.
// Keep this explicit while the legacy iroh plugin remains an optional rollback.
repositories {
    maven {
        url =
            uri(
                "https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/",
            )
        content { includeGroup("org.rustls") }
    }
}

dependencies { implementation("org.rustls:rustls-platform-verifier:0.2.0") }
