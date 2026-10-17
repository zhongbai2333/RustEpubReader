package com.zhongbai233.epub.reader.util

import android.util.Log
import android.os.Build
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * GitHub Release 更新检查器。
 * 通过 GitHub API 获取最新 release，与当前版本比较。
 * Android 端不进行自更新，仅提示用户跳转下载。
 */
object UpdateChecker {
    private const val TAG = "UpdateChecker"
    private const val OWNER = "zhongbai2333"
    private const val REPO = "RustEpubReader"
    private const val API_URL = "https://api.github.com/repos/$OWNER/$REPO/releases/latest"

    data class UpdateInfo(
        val tagName: String,
        val releaseName: String,
        val cdnDownloadUrl: String,
        val githubDownloadUrl: String
    )

    /**
     * 检查是否有新版本。
     * @param currentVersion 当前版本号，如 "1.0.0"
     * @return UpdateInfo if a new version is available, null otherwise
     */
    suspend fun checkForUpdate(currentVersion: String): UpdateInfo? = withContext(Dispatchers.IO) {
        try {
            val conn = URL(API_URL).openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            conn.setRequestProperty("User-Agent", "RustEpubReader-Android/1.0")
            conn.connectTimeout = 10_000
            conn.readTimeout = 10_000

            if (conn.responseCode != 200) {
                Log.w(TAG, "GitHub API returned ${conn.responseCode}")
                return@withContext null
            }

            val body = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()

            val json = JSONObject(body)
            val tagName = json.optString("tag_name", "")
            val releaseName = json.optString("name", "")

            if (json.optBoolean("draft") || json.optBoolean("prerelease") || !isNewerRelease(tagName, currentVersion)) {
                return@withContext null
            }

            val releasePageUrl = "https://github.com/$OWNER/$REPO/releases/tag/$tagName"

            // 查找 Android APK 资产
            val assets = json.optJSONArray("assets") ?: return@withContext null
            val candidates = (0 until assets.length()).map { i ->
                val asset = assets.getJSONObject(i)
                asset.optString("name", "") to asset.optString("browser_download_url", "")
            }
            val githubDownloadUrl = matchingApk(candidates, Build.SUPPORTED_ABIS.toList())

            val resolvedGithubUrl = githubDownloadUrl?.takeIf { it.isNotBlank() } ?: releasePageUrl
            val resolvedCdnUrl = getAcceleratedUrl(resolvedGithubUrl)

            UpdateInfo(
                tagName = tagName,
                releaseName = releaseName,
                cdnDownloadUrl = resolvedCdnUrl,
                githubDownloadUrl = resolvedGithubUrl
            )
        } catch (e: Exception) {
            Log.e(TAG, "Failed to check for updates", e)
            null
        }
    }

    private fun getAcceleratedUrl(originalUrl: String): String {
        val marker = "/releases/download/"
        val idx = originalUrl.indexOf(marker)
        if (idx >= 0) {
            val tail = originalUrl.substring(idx + marker.length)
            return "https://dl.zhongbai233.com/release/$tail"
        }
        return originalUrl
    }
}
