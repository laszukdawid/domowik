plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "invalid.domowik_spike.domowik_spike_app"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        // An EXPERIMENT-LOCAL identifier, not a claim on a name.
        //
        // An Android applicationId is an identity, and docs/NAMING.md forbids
        // asserting one while `domowik.dev` is registered by a third party
        // (dossier O4); issue 0034 settles the real identity. This uses the
        // RFC 2606 `.invalid` top-level domain, which can never be delegated to
        // anyone, exactly as the macOS bundle identifier does. The separators
        // differ from the macOS spelling only because an applicationId must be
        // a legal Java package name and cannot contain a hyphen.
        applicationId = "invalid.domowik_spike.domowik_spike_app"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName

        // NO `ndk { abiFilters }` here, and that absence is a finding rather
        // than an omission.
        //
        // It was tried first, with a comment asserting it restricted the APK to
        // arm64-v8a. The built APK disproved that: `libdomowik_bridge.so` was
        // arm64-only because the plugin builds one ABI, while Flutter's own
        // `libflutter.so` and `libapp.so` were packaged for armeabi-v7a and
        // x86_64 as well. The Flutter Gradle plugin places its engine and AOT
        // libraries from the tool's target-platform list, which `abiFilters`
        // does not govern -- so the setting would have produced an APK that
        // installs on an armeabi-v7a device and then fails at
        // DynamicLibrary.open, which is worse than not filtering at all.
        //
        // The ABI is selected where Flutter actually decides it, by passing
        // `--target-platform android-arm64`; `scripts/android-build.sh` passes
        // it and then ASSERTS that the APK carries exactly one ABI directory,
        // because that is the property that silently stops being true.
    }

    buildTypes {
        release {
            // The debug keystore, deliberately and with the same reasoning
            // ED-0099 applied to macOS: this is a local-development target and
            // nothing here is distributed, so there is no release key to sign
            // with and no store to sign for. What that costs is recorded rather
            // than glossed: the APK measured by this spike is signed by the
            // auto-generated debug key, so it is installable on an emulator and
            // on a developer's own device and is not a distributable artifact.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}
