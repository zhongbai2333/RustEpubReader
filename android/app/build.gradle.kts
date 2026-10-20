import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.OutputDirectory

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val readerNativeProfile = System.getenv("READER_RUST_PROFILE") ?: "release"
require(readerNativeProfile in setOf("release", "release-fast")) { "Unsupported READER_RUST_PROFILE" }

android {
    namespace = "com.zhongbai233.epub.reader"
    compileSdk = 35

    defaultConfig {
        applicationId = project.findProperty("READER_DIAGNOSTIC_APPLICATION_ID") as String? ?: "com.zhongbai233.epub.reader"
        testInstrumentationRunner = "com.zhongbai233.epub.reader.ui.reader.IllustrationInstrumentation"
        minSdk = 26
        targetSdk = 35
        versionCode = (project.findProperty("APP_VERSION_CODE") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("APP_VERSION_NAME") as String?) ?: "1.0.0"
        // Expose version to Kotlin code via BuildConfig
        buildConfigField("String", "APP_VERSION_NAME", "\"${versionName}\"")
        buildConfigField("String", "READER_NATIVE_PROFILE", "\"$readerNativeProfile\"")
        buildConfigField("boolean", "READER_PERFORMANCE_CHECKS", project.hasProperty("READER_PERFORMANCE_CHECKS").toString())

        // NOTE: We do NOT set `ndk.abiFilters` here on purpose. AGP 8 + Gradle 9
        // refuses to start when defaultConfig.ndk.abiFilters and splits.abi
        // filters are both set ("Conflicting configuration"). The ABI list is
        // declared once in the `splits { abi { include(...) } }` block below;
        // that single source of truth controls both packaging and which native
        // libs ship in each per-ABI APK.
    }

    signingConfigs {
        val keystoreFile = System.getenv("ANDROID_KEYSTORE_FILE")
            ?: project.findProperty("ANDROID_KEYSTORE_FILE") as String?
        val ksAlias = System.getenv("ANDROID_KEY_ALIAS")
            ?: project.findProperty("ANDROID_KEY_ALIAS") as String?
        val ksPassword = System.getenv("ANDROID_STORE_PASSWORD")
            ?: project.findProperty("ANDROID_STORE_PASSWORD") as String?
        val kPassword = System.getenv("ANDROID_KEY_PASSWORD")
            ?: project.findProperty("ANDROID_KEY_PASSWORD") as String?

        if (!keystoreFile.isNullOrBlank() && !ksAlias.isNullOrBlank()) {
            create("release") {
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
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.findByName("release")
                ?: signingConfigs.getByName("debug")
            ndk {
                debugSymbolLevel = "NONE"
            }
        }
    }

    // Produce one APK per ABI (armeabi-v7a, arm64-v8a, x86_64) instead of a fat universal APK.
    // This roughly halves the install size for end users.
    splits {
        abi {
            isEnable = true
            reset()
            include("armeabi-v7a", "arm64-v8a", "x86_64")
            isUniversalApk = false
        }
    }

    // Explicit opt-in diagnostic build; shipping releases do not contain this activity.
    if (project.hasProperty("READER_PERFORMANCE_CHECKS")) {
        sourceSets.getByName("release").kotlin.srcDir("src/performance/java")
        sourceSets.getByName("release").manifest.srcFile("src/performance/AndroidManifest.xml")
    }
    sourceSets.getByName("androidTest").assets.srcDir("../../core/tests/fixtures")
    sourceSets.getByName("test").resources.srcDir("../../core/tests/fixtures")
    sourceSets.getByName("main").assets.srcDir("../../resources")

    packaging {
        jniLibs {
            useLegacyPackaging = false
            // Ask R8/AGP to keep .so files compressed-and-page-aligned only when needed.
            // Cargo already strips this library. Keep its exact verified bytes.
            keepDebugSymbols += "**/libandroid_bridge.so"
            // Do NOT bundle ONNX Runtime native libs in the APK. They are
            // downloaded on-demand to filesDir and dlopen'd by CscNativeLoader.
            // This trims ~25 MB per ABI from the install size.
            excludes += listOf(
                "**/libonnxruntime.so",
                "**/libonnxruntime4j_jni.so"
            )
        }
        resources {
            excludes += listOf(
                "META-INF/*.kotlin_module",
                "META-INF/AL2.0",
                "META-INF/LGPL2.1",
                "META-INF/DEPENDENCIES",
                "META-INF/LICENSE",
                "META-INF/LICENSE.txt",
                "META-INF/LICENSE.md",
                "META-INF/NOTICE",
                "META-INF/NOTICE.txt",
                "META-INF/NOTICE.md",
                "META-INF/*.version",
                "**/*.kotlin_metadata",
                "kotlin-tooling-metadata.json"
            )
        }
    }

    bundle {
        language { enableSplit = true }
        density { enableSplit = true }
        abi { enableSplit = true }
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
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")

    // Coil for image loading
    implementation("io.coil-kt:coil-compose:2.7.0")

    // OkHttp for Edge TTS WebSocket
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // ONNX Runtime for CSC model inference
    implementation("com.microsoft.onnxruntime:onnxruntime-android:1.24.3")

    // Realistic page curl animation (vendored source for customization)
    implementation(project(":pagecurl"))

    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
}

// Publish the selected Cargo artifacts by content. cargo-ndk 4.1.2's -o
// freshness check uses only mtime, which can skip an older cached profile.
@org.gradle.work.DisableCachingByDefault(because = "Cargo manages native incremental state")
abstract class ReaderCargoBuild : Exec() {
    @get:OutputDirectory abstract val jniDirectory: DirectoryProperty
    @get:OutputDirectory abstract val provenanceDirectory: DirectoryProperty
}
val rustNativeAssets = layout.buildDirectory.dir("generated/rustNativeAssets/$readerNativeProfile")
val rustNativePublisher = rootProject.file("../scripts/android-native-artifacts.py")
fun runRustArtifactTool(vararg arguments: String) {
    val process = ProcessBuilder(listOf("python3", rustNativePublisher.absolutePath) + arguments)
        .directory(rootProject.projectDir.parentFile).redirectErrorStream(true).start()
    val output = process.inputStream.bufferedReader().use { it.readText() }
    check(process.waitFor() == 0) { "Rust native artifact verification failed:\n$output" }
    if (output.isNotBlank()) logger.lifecycle(output.trimEnd())
}
val cargoBuild by tasks.registering(ReaderCargoBuild::class) {
    jniDirectory.set(layout.buildDirectory.dir("generated/rustJni/$readerNativeProfile"))
    provenanceDirectory.set(rustNativeAssets)
    workingDir = file("../../android-bridge")
    commandLine(
        "cargo", "ndk",
        "-t", "armeabi-v7a", "-t", "arm64-v8a", "-t", "x86_64",
        "build", "--profile", readerNativeProfile, "--locked"
    )
    inputs.property("rustProfile", readerNativeProfile)
    // Cargo owns Rust dependency/incremental checks. Declared outputs also
    // invalidate Gradle's file snapshots before publishing cached libraries.
    outputs.upToDateWhen { false }
    doLast {
        runRustArtifactTool("publish", "--profile", readerNativeProfile,
            "--output", jniDirectory.get().asFile.absolutePath,
            "--assets", provenanceDirectory.get().asFile.absolutePath)
    }
}
// Old source-tree JNI files are retained as caches, not packaged as inputs.
android.sourceSets["main"].jniLibs.setSrcDirs(emptyList<String>())
androidComponents.onVariants { variant ->
    variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild) { it.jniDirectory }
    variant.sources.assets?.addGeneratedSourceDirectory(cargoBuild) { it.provenanceDirectory }
}
tasks.configureEach {
    if (name == "assembleDebug" || name == "assembleRelease") {
        val variant = if (name == "assembleDebug") "debug" else "release"
        doLast {
            runRustArtifactTool("verify-outputs", "--profile", readerNativeProfile,
                "--metadata", layout.buildDirectory.file("outputs/apk/$variant/output-metadata.json").get().asFile.absolutePath)
        }
    }
}

// Both native UIs consume the exact core catalog source; generated assets are not edited.
val readerCatalogs by tasks.registering(Sync::class) {
    from(rootProject.file("../core/src/i18n")) {include("en.json", "zh_cn.json")}
    into(layout.buildDirectory.dir("generated/readerCatalogs/i18n"))
}
android.sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/readerCatalogs").get().asFile)
tasks.named("preBuild") {dependsOn(readerCatalogs)}

val validateReaderTranslations by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir.parentFile)
    commandLine("python3", "scripts/check-i18n.py")
}
readerCatalogs { dependsOn(validateReaderTranslations) }

// All native clients validate the same persisted-coordinate contract vectors.
tasks.withType<org.gradle.api.tasks.testing.Test>().configureEach {
    systemProperty("reader.coordinateContract", rootProject.file("../tests/fixtures/reader-coordinates.json").absolutePath)
}
