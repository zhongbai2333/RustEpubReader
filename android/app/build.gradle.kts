plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "com.zhongbai233.epub.reader"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.zhongbai233.epub.reader"
        minSdk = 26
        targetSdk = 35
        versionCode = (project.findProperty("APP_VERSION_CODE") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("APP_VERSION_NAME") as String?) ?: "1.0.0"
        // Expose version to Kotlin code via BuildConfig
        buildConfigField("String", "APP_VERSION_NAME", "\"${versionName}\"")
    }

    signingConfigs {
        create("release") {
            val keystoreFile = System.getenv("ANDROID_KEYSTORE_FILE")
                ?: project.findProperty("ANDROID_KEYSTORE_FILE") as String?
            val ksAlias = System.getenv("ANDROID_KEY_ALIAS")
                ?: project.findProperty("ANDROID_KEY_ALIAS") as String?
            val ksPassword = System.getenv("ANDROID_STORE_PASSWORD")
                ?: project.findProperty("ANDROID_STORE_PASSWORD") as String?
            val kPassword = System.getenv("ANDROID_KEY_PASSWORD")
                ?: project.findProperty("ANDROID_KEY_PASSWORD") as String?

            if (keystoreFile != null && ksAlias != null) {
                storeFile = file(keystoreFile)
                keyAlias = ksAlias
                storePassword = ksPassword
                keyPassword = kPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.findByName("release")
                ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    // Compose BOM
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)

    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.activity:activity-compose:1.9.3")

    // Compose
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.foundation:foundation")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.8.5")

    // ViewModel
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")

    // Serialization
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    // HTML parsing
    implementation("org.jsoup:jsoup:1.18.3")

    // Coil for image loading
    implementation("io.coil-kt:coil-compose:2.7.0")

    // Realistic page curl animation (vendored source for customization)
    implementation(project(":pagecurl"))

    debugImplementation("androidx.compose.ui:ui-tooling")
}

// Cargo NDK Integration
tasks.register<Exec>("cargoBuild") {
    // Requires cargo-ndk to be installed globally on developer's machine:
    // cargo install cargo-ndk
    // rustup target add aarch64-linux-android x86_64-linux-android
    workingDir = file("../../android-bridge")
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", 
        "-t", "x86_64",
        "-o", "../android/app/src/main/jniLibs",
        "build", "--release"
    )
}

tasks.whenTaskAdded {
    if (name.startsWith("merge") && name.endsWith("JniLibFolders")) {
        dependsOn("cargoBuild")
    }
}
