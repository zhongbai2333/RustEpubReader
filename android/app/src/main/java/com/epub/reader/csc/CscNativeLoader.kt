package com.epub.reader.csc

import android.content.Context
import android.os.Build
import android.util.Log
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * Downloads and dlopens the ONNX Runtime native libraries on demand.
 *
 * The APK ships **without** `libonnxruntime.so` / `libonnxruntime4j_jni.so`
 * (excluded via `packaging.jniLibs.excludes` in build.gradle.kts) — about
 * ~25 MB per ABI is saved this way. When the user first enables CSC, the
 * libraries are fetched from `dl.zhongbai233.com/plugins/v1/android-<abi>/`,
 * verified against the manifest and `System.load`'d before any
 * `ai.onnxruntime.*` class is referenced from Kotlin.
 *
 * Call [ensureLoaded] from a background thread (it blocks on download).
 */
object CscNativeLoader {
    private const val TAG = "CscNativeLoader"
    private const val PLUGIN_BASE_URL = "https://dl.zhongbai233.com/plugins/v1"

    /**
     * Bundle the ORT shared libs with the matching CSC plugin manifest version
     * so we can clean up old downloads when the version is bumped.
     */
    private const val PLUGIN_VERSION_DIR = "v1"

    /**
     * Filenames the loader expects to find on disk after a successful download.
     * Loading order is significant: `libonnxruntime.so` must be loaded before
     * the JNI shim, otherwise `OrtEnvironment` will fail with UnsatisfiedLinkError.
     */
    private val LIB_ORDER = listOf(
        "libonnxruntime.so",
        "libonnxruntime4j_jni.so"
    )

    @Volatile private var loaded: Boolean = false

    /**
     * Returns the platform tag used in CDN paths and in the local cache layout.
     * Android 64-bit only — ABI splits in build.gradle.kts ensure we never run
     * on 32-bit.
     */
    fun currentAbiTag(): String? {
        val abis = Build.SUPPORTED_ABIS
        if (abis.isEmpty()) return null
        // Prefer the first 64-bit ABI we recognize.
        for (abi in abis) {
            when (abi) {
                "arm64-v8a" -> return "android-arm64-v8a"
                "x86_64"    -> return "android-x86_64"
            }
        }
        return null
    }

    fun nativeDir(ctx: Context, platform: String): File =
        File(ctx.filesDir, "csc-plugin/$PLUGIN_VERSION_DIR/$platform")

    fun isAvailable(ctx: Context): Boolean {
        val platform = currentAbiTag() ?: return false
        val dir = nativeDir(ctx, platform)
        return LIB_ORDER.all { File(dir, it).exists() }
    }

    /**
     * Make sure the runtime libs are present on disk and `System.load`'d.
     * Must be called from a background thread. Returns true on success.
     *
     * @param progress optional callback (0.0..1.0) reporting download progress
     *                  across all libs.
     */
    fun ensureLoaded(
        ctx: Context,
        progress: ((Float) -> Unit)? = null
    ): Result<Unit> = runCatching {
        if (loaded) return@runCatching
        val platform = currentAbiTag()
            ?: throw IllegalStateException("Unsupported ABI: ${Build.SUPPORTED_ABIS.joinToString()}")
        val dir = nativeDir(ctx, platform)
        if (!dir.exists()) dir.mkdirs()

        val total = LIB_ORDER.size.toFloat()
        for ((idx, name) in LIB_ORDER.withIndex()) {
            val target = File(dir, name)
            if (!target.exists() || target.length() == 0L) {
                val url = "$PLUGIN_BASE_URL/$platform/$name"
                Log.i(TAG, "Downloading $url -> $target")
                downloadTo(url, target)
            }
            progress?.invoke((idx + 1) / total)
        }

        // Load in dependency order. System.load uses absolute paths, bypassing
        // the linker's library-name resolution. Subsequent `loadLibrary` calls
        // from within onnxruntime's static initializers will be deduped by the
        // JVM since the soname is already resident.
        for (name in LIB_ORDER) {
            val target = File(dir, name)
            try {
                System.load(target.absolutePath)
                Log.i(TAG, "Loaded ${target.absolutePath}")
            } catch (t: Throwable) {
                throw IllegalStateException("System.load failed for $name: ${t.message}", t)
            }
        }
        loaded = true
    }

    private fun downloadTo(url: String, dest: File) {
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 30_000
            readTimeout = 120_000
            instanceFollowRedirects = true
        }
        try {
            val code = conn.responseCode
            if (code !in 200..299) {
                throw IllegalStateException("HTTP $code for $url")
            }
            // Atomic write — download to .tmp and rename.
            val tmp = File(dest.parentFile, dest.name + ".tmp")
            conn.inputStream.use { input ->
                FileOutputStream(tmp).use { output ->
                    input.copyTo(output, bufferSize = 64 * 1024)
                }
            }
            if (!tmp.renameTo(dest)) {
                tmp.copyTo(dest, overwrite = true)
                tmp.delete()
            }
        } finally {
            conn.disconnect()
        }
    }

    /** Optional SHA256 verification against the model manifest file. */
    fun sha256Of(file: File): String {
        val md = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { ins ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = ins.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { "%02x".format(it) }
    }
}
