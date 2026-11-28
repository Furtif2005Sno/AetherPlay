plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "fr.jolannrt.aetherplay"
    compileSdk = 36

    defaultConfig {
        applicationId = "fr.jolannrt.aetherplay"
        // BluetoothHidDevice (the phone as a Bluetooth controller) needs Android 9.
        minSdk = 28
        targetSdk = 36
        // Release builds take the release's number (CI: APP_VERSION from the
        // tag, "0.6.1" -> code 601), so the app never offers its own version as
        // an update; other builds keep these.
        val release = System.getenv("APP_VERSION")?.removePrefix("v")?.takeIf { Regex("""\d+\.\d+\.\d+""").matches(it) }
        versionCode = release?.split('.')?.map(String::toInt)?.let { (a, b, c) -> a * 10000 + b * 100 + c } ?: 17
        versionName = release ?: "0.5.4"
    }

    signingConfigs {
        // CI signs with a key from secrets; local builds fall back to the debug key.
        create("release") {
            val ks = System.getenv("ANDROID_KEYSTORE")
            if (ks != null) {
                storeFile = file(ks)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (System.getenv("ANDROID_KEYSTORE") != null) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.10.00")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20250517")
}
