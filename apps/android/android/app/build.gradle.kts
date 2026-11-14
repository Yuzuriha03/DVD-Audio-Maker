plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val localNdk = System.getenv("DVDA_ANDROID_NDK")

android {
    namespace = "com.dvda.maker"
    compileSdk = 37
    if (!localNdk.isNullOrBlank()) {
        ndkPath = localNdk
    } else {
        ndkVersion = flutter.ndkVersion
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
            excludes += setOf(
                "armeabi-v7a/**", "x86/**", "x86_64/**",
                "lib/armeabi-v7a/**", "lib/x86/**", "lib/x86_64/**",
            )
        }
    }

    defaultConfig {
        applicationId = "com.dvda.maker"
        minSdk = 24
        targetSdk = 37
        // Uses the version code from pubspec.yaml.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    buildTypes {
        release {
            // Signed with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug").apply {
                enableV1Signing = true
                enableV2Signing = true
            }
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

val stripReleaseNativeSymbols = tasks.register("stripReleaseNativeSymbols") {
    dependsOn("mergeReleaseJniLibFolders", "stripReleaseDebugSymbols")
    onlyIf { !localNdk.isNullOrBlank() }
    val mergedDirs = listOf(
        "intermediates/merged_jni_libs/release/mergeReleaseJniLibFolders",
        "intermediates/stripped_native_libs/release/stripReleaseDebugSymbols",
    ).map { layout.buildDirectory.dir(it) }
    outputs.upToDateWhen { false }
    doLast {
        val strip = file("$localNdk/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-strip.exe")
        if (!strip.isFile) {
            throw GradleException("llvm-strip not found: $strip")
        }
        val existingDirs = mergedDirs.map { it.get().asFile }.filter { it.isDirectory }
        if (existingDirs.isEmpty()) {
            throw GradleException("No merged JNI directories found")
        }
        existingDirs.forEach { directory ->
            directory.walkTopDown()
            .filter { it.isFile && it.extension == "so" }
            .forEach { library ->
                library.setWritable(true)
                val process = ProcessBuilder(
                    strip.absolutePath,
                    "--strip-unneeded",
                    library.absolutePath,
                ).redirectErrorStream(true).start()
                val output = process.inputStream.bufferedReader().readText()
                if (process.waitFor() != 0) {
                    throw GradleException("llvm-strip failed for $library: $output")
                }
            }
        }
    }
}

tasks.matching { it.name == "packageRelease" }.configureEach {
    dependsOn(stripReleaseNativeSymbols)
}
