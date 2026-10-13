package com.zhongbai233.epub.reader.util

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
    suspend fun submitContribution(
        token: String,
        username: String,
        jsonl: String
    ): ContributeResult = withContext(Dispatchers.IO) {
        val auth = "Bearer $token"
        val ua = "RustEpubReader-Android/1.0"
        val timestamp = System.currentTimeMillis() / 1000

        try {
            // Step 1: Fork
            Log.d(TAG, "Step 1: forking repo")
            val forkConn = URL("https://api.github.com/repos/$MODEL_REPO_OWNER/$MODEL_REPO_NAME/forks")
                .openConnection() as HttpURLConnection
            forkConn.requestMethod = "POST"
            forkConn.setRequestProperty("Authorization", auth)
            forkConn.setRequestProperty("User-Agent", ua)
            forkConn.setRequestProperty("Accept", "application/vnd.github+json")
            forkConn.setRequestProperty("Content-Length", "0")
            forkConn.doOutput = true
            forkConn.outputStream.close()
            val forkStatus = forkConn.responseCode
            forkConn.disconnect()
            Log.d(TAG, "Fork response: $forkStatus")

            if (forkStatus !in listOf(200, 201, 202)) {
                // 403 can mean fork already exists, continue anyway
                if (forkStatus != 403) {
                    // Try to continue, fork may already exist
                }
            }

            // Small delay to let GitHub process the fork
            delay(2000)

            // Get default branch
            val repoConn = URL("https://api.github.com/repos/$username/$MODEL_REPO_NAME")
                .openConnection() as HttpURLConnection
            repoConn.setRequestProperty("Authorization", auth)
            repoConn.setRequestProperty("User-Agent", ua)
            repoConn.setRequestProperty("Accept", "application/vnd.github+json")
            val defaultBranch = if (repoConn.responseCode == 200) {
                val json = JSONObject(repoConn.inputStream.bufferedReader().use { it.readText() })
                json.optString("default_branch", "main")
            } else {
                "main"
            }
            repoConn.disconnect()

            // Step 2: Create file
            Log.d(TAG, "Step 2: creating file on fork")
            val filePath = "$SUBMISSION_DIR/${username}_${timestamp}.jsonl"
            val contentB64 = Base64.encodeToString(jsonl.toByteArray(), Base64.NO_WRAP)

            val commitBody = JSONObject().apply {
                put("message", "Add correction data from $username")
                put("content", contentB64)
                put("branch", defaultBranch)
            }

            val createConn = URL("https://api.github.com/repos/$username/$MODEL_REPO_NAME/contents/$filePath")
                .openConnection() as HttpURLConnection
            createConn.requestMethod = "PUT"
            createConn.setRequestProperty("Authorization", auth)
            createConn.setRequestProperty("User-Agent", ua)
            createConn.setRequestProperty("Accept", "application/vnd.github+json")
            createConn.setRequestProperty("Content-Type", "application/json")
            createConn.doOutput = true
            OutputStreamWriter(createConn.outputStream).use { it.write(commitBody.toString()) }

            val createStatus = createConn.responseCode
            Log.d(TAG, "Create file response: $createStatus")

            if (createStatus !in listOf(200, 201)) {
                // If file exists (409/422), try updating with SHA
                if (createStatus == 422 || createStatus == 409) {
                    val updated = tryUpdateExistingFile(
                        auth, ua, username, filePath, contentB64, defaultBranch
                    )
                    if (!updated) {
                        return@withContext ContributeResult.Error("Failed to create/update file: HTTP $createStatus")
                    }
                } else {
                    val errBody = try { createConn.errorStream?.bufferedReader()?.use { it.readText() } } catch (_: Exception) { null as String? }
                    createConn.disconnect()
                    return@withContext ContributeResult.Error("Create file failed: HTTP $createStatus ${errBody ?: ""}")
                }
            }
            createConn.disconnect()

            // Step 3: Create PR
            Log.d(TAG, "Step 3: creating pull request")
            val prBody = JSONObject().apply {
                put("title", "CSC correction data from $username")
                put("body", "Automatically submitted correction data from RustEpubReader Android.\n\n" +
                    "- User: @$username\n- Samples: ${jsonl.lines().count { it.isNotBlank() }}\n- File: `$filePath`")
                put("head", "$username:$defaultBranch")
                put("base", defaultBranch)
            }

            val prConn = URL("https://api.github.com/repos/$MODEL_REPO_OWNER/$MODEL_REPO_NAME/pulls")
                .openConnection() as HttpURLConnection
            prConn.requestMethod = "POST"
            prConn.setRequestProperty("Authorization", auth)
            prConn.setRequestProperty("User-Agent", ua)
            prConn.setRequestProperty("Accept", "application/vnd.github+json")
            prConn.setRequestProperty("Content-Type", "application/json")
            prConn.doOutput = true
            OutputStreamWriter(prConn.outputStream).use { it.write(prBody.toString()) }

            val prStatus = prConn.responseCode
            Log.d(TAG, "Create PR response: $prStatus")

            if (prStatus in listOf(200, 201)) {
                val prJson = JSONObject(prConn.inputStream.bufferedReader().use { it.readText() })
                prConn.disconnect()
                val prUrl = prJson.optString("html_url", "")
                return@withContext ContributeResult.Success(prUrl)
            } else if (prStatus == 422) {
                // PR may already exist
                prConn.disconnect()
                val existingUrl = findExistingPr(auth, ua, username, defaultBranch)
                return@withContext ContributeResult.Success(
                    existingUrl ?: "https://github.com/$MODEL_REPO_OWNER/$MODEL_REPO_NAME/pulls"
                )
            } else {
                val errBody = try { prConn.errorStream?.bufferedReader()?.use { it.readText() } } catch (_: Exception) { null as String? }
                prConn.disconnect()
                return@withContext ContributeResult.Error("Create PR failed: HTTP $prStatus ${errBody ?: ""}")
            }
        } catch (cancelled: CancellationException) { throw cancelled } catch (e: Exception) {
            Log.e(TAG, "submitContribution failed", e)
            ContributeResult.Error(e.message ?: "Unknown error")
        }
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
