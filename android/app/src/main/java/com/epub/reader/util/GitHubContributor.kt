package com.zhongbai233.epub.reader.util

import com.zhongbai233.epub.reader.i18n.I18n
import android.util.Base64
import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL

/**
 * GitHub Device Flow OAuth + Contribution submission.
 * Uses the same CLIENT_ID as the desktop app.
 */
object GitHubContributor {
    private const val TAG = "GitHubContributor"
    private const val CLIENT_ID = "Ov23liG7iXNfGOTAXXxx"
    private const val MODEL_REPO_OWNER = "zhongbai2333"
    private const val MODEL_REPO_NAME = "RustEpubReader-Model"
    private const val SUBMISSION_DIR = "datasets/submissions"

    // ── Device Flow OAuth ──

    data class DeviceCode(
        val deviceCode: String,
        val userCode: String,
        val verificationUri: String,
        val expiresIn: Int,
        val interval: Int
    )

    sealed class AuthResult {
        data class Success(val token: String, val username: String, val repoAuthorized: Boolean) : AuthResult()
        data class Error(val message: String) : AuthResult()
    }

    /**
     * Step 1: Request device + user codes from GitHub.
     */
    suspend fun requestDeviceCode(privateSync: Boolean = false): Result<DeviceCode> = withContext(Dispatchers.IO) {
        try {
            val conn = URL("https://github.com/login/device/code").openConnection() as HttpURLConnection
            conn.connectTimeout = 30_000
            conn.readTimeout = 30_000
            conn.instanceFollowRedirects = false
            conn.requestMethod = "POST"
            conn.setRequestProperty("Accept", "application/json")
            conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
            conn.doOutput = true

            val body = "client_id=$CLIENT_ID&scope=${if (privateSync) "repo" else "public_repo"}"
            OutputStreamWriter(conn.outputStream).use { it.write(body) }

            if (conn.responseCode != 200) {
                return@withContext Result.failure(Exception("HTTP ${conn.responseCode}"))
            }

            val json = JSONObject(conn.inputStream.bufferedReader().use { it.readText() })
            conn.disconnect()

            Result.success(
                DeviceCode(
                    deviceCode = json.getString("device_code"),
                    userCode = json.getString("user_code"),
                    verificationUri = json.optString("verification_uri", "https://github.com/login/device"),
                    expiresIn = json.optInt("expires_in", 900),
                    interval = json.optInt("interval", 5)
                )
            )
        } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
            Log.e(TAG, "requestDeviceCode failed", e)
            Result.failure(e)
        }
    }

    /**
     * Step 2: Poll for access token until user authorizes, expires, or denies.
     */
    suspend fun pollForToken(deviceCode: DeviceCode): AuthResult = withContext(Dispatchers.IO) {
        val deadline = System.currentTimeMillis() + deviceCode.expiresIn * 1000L
        var interval = deviceCode.interval * 1000L

        while (System.currentTimeMillis() < deadline) {
            delay(interval)

            try {
                val conn = URL("https://github.com/login/oauth/access_token").openConnection() as HttpURLConnection
                conn.connectTimeout = 30_000
                conn.readTimeout = 30_000
                conn.instanceFollowRedirects = false
                conn.requestMethod = "POST"
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
                conn.doOutput = true

                val body = "client_id=$CLIENT_ID&device_code=${deviceCode.deviceCode}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
                OutputStreamWriter(conn.outputStream).use { it.write(body) }

                val json = JSONObject(conn.inputStream.bufferedReader().use { it.readText() })
                conn.disconnect()

                val error = json.optString("error", "")
                when {
                    json.has("access_token") -> {
                        val token = json.getString("access_token")
                        val account = fetchAccount(token) ?: return@withContext AuthResult.Error("Cannot verify GitHub account")
                        return@withContext AuthResult.Success(token, account.first, account.second)
                    }
                    error == "authorization_pending" -> continue
                    error == "slow_down" -> {
                        interval += 5000L
                        continue
                    }
                    error == "expired_token" -> return@withContext AuthResult.Error("Code expired")
                    error == "access_denied" -> return@withContext AuthResult.Error("Access denied")
                    else -> return@withContext AuthResult.Error("OAuth error: $error")
                }
            } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
                Log.e(TAG, "pollForToken error", e)
            }
        }

        AuthResult.Error("Polling timed out")
    }

    private fun fetchAccount(token: String): Pair<String, Boolean>? {
        return try {
            val conn = URL("https://api.github.com/user").openConnection() as HttpURLConnection
            conn.connectTimeout = 30_000
            conn.readTimeout = 30_000
            conn.instanceFollowRedirects = false
            conn.setRequestProperty("Authorization", "Bearer $token")
            conn.setRequestProperty("User-Agent", "RustEpubReader-Android/1.0")
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            if (conn.responseCode != 200) { conn.disconnect(); return null }
            val repoAuthorized = hasGithubRepoScope(conn.getHeaderField("X-OAuth-Scopes") ?: "")
            val json = JSONObject(conn.inputStream.bufferedReader().use { it.readText() })
            conn.disconnect()
            json.optString("login").takeIf { it.isNotBlank() }?.let { it to repoAuthorized }
        } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
            Log.e(TAG, "fetchUsername failed", e)
            null
        }
    }

    // ── Contribution Submission ──

    sealed class ContributeResult {
        data class Success(val prUrl: String) : ContributeResult()
        data class Error(val message: String) : ContributeResult()
    }

    /**
     * Submit JSONL data to the model repo by:
     * 1. Fork the repo (no-op if already forked)
     * 2. Create file on the fork
     * 3. Open PR to upstream
     */
    suspend fun submitContribution(token:String,username:String,jsonl:String):ContributeResult=withContext(Dispatchers.IO){
        try{
            val samples=JSONArray();jsonl.lineSequence().filter{it.isNotBlank()}.forEach{samples.put(JSONObject(it))}
            val request=JSONObject().put("op","csc_submit").put("token",token).put("samples",samples)
            val envelope=JSONObject(com.zhongbai233.epub.reader.RustBridge.readingTools(request.toString())?:error("No contribution response"))
            check(envelope.optBoolean("ok")){envelope.optString("error")};ContributeResult.Success(envelope.getString("result"))
        }catch(e:CancellationException){throw e}catch(e:Exception){ContributeResult.Error(e.message?:I18n.t("common.error_unknown"))}
    }

    private fun tryUpdateExistingFile(
        auth: String,
        ua: String,
        username: String,
        filePath: String,
        contentB64: String,
        branch: String
    ): Boolean {
        return try {
            val getConn = URL("https://api.github.com/repos/$username/$MODEL_REPO_NAME/contents/$filePath?ref=$branch")
                .openConnection() as HttpURLConnection
            getConn.setRequestProperty("Authorization", auth)
            getConn.setRequestProperty("User-Agent", ua)
            getConn.setRequestProperty("Accept", "application/vnd.github+json")

            if (getConn.responseCode != 200) {
                getConn.disconnect()
                return false
            }

            val json = JSONObject(getConn.inputStream.bufferedReader().use { it.readText() })
            getConn.disconnect()
            val sha = json.optString("sha").takeIf { it.isNotBlank() } ?: return false

            val updateBody = JSONObject().apply {
                put("message", "Update correction data from $username")
                put("content", contentB64)
                put("sha", sha)
                put("branch", branch)
            }

            val putConn = URL("https://api.github.com/repos/$username/$MODEL_REPO_NAME/contents/$filePath")
                .openConnection() as HttpURLConnection
            putConn.requestMethod = "PUT"
            putConn.setRequestProperty("Authorization", auth)
            putConn.setRequestProperty("User-Agent", ua)
            putConn.setRequestProperty("Accept", "application/vnd.github+json")
            putConn.setRequestProperty("Content-Type", "application/json")
            putConn.doOutput = true
            OutputStreamWriter(putConn.outputStream).use { it.write(updateBody.toString()) }

            val ok = putConn.responseCode in listOf(200, 201)
            putConn.disconnect()
            ok
        } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
            Log.e(TAG, "tryUpdateExistingFile failed", e)
            false
        }
    }

    private fun findExistingPr(auth: String, ua: String, username: String, branch: String): String? {
        return try {
            val conn = URL("https://api.github.com/repos/$MODEL_REPO_OWNER/$MODEL_REPO_NAME/pulls?head=$username:$branch&state=open")
                .openConnection() as HttpURLConnection
            conn.setRequestProperty("Authorization", auth)
            conn.setRequestProperty("User-Agent", ua)
            conn.setRequestProperty("Accept", "application/vnd.github+json")

            if (conn.responseCode != 200) {
                conn.disconnect()
                return null
            }

            val arr = JSONArray(conn.inputStream.bufferedReader().use { it.readText() })
            conn.disconnect()
            if (arr.length() > 0) {
                arr.getJSONObject(0).optString("html_url").takeIf { it.isNotBlank() }
            } else null
        } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
            Log.e(TAG, "findExistingPr failed", e)
            null
        }
    }
}
