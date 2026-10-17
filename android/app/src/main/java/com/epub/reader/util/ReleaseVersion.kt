package com.zhongbai233.epub.reader.util

/** Only stable numeric release tags are eligible for update notices. */
internal fun isNewerRelease(remote: String, current: String): Boolean {
    fun parse(raw: String): List<Long>? {
        val parts = raw.removePrefix("v").split('.')
        if (parts.size != 3 || parts.any { !it.matches(Regex("0|[1-9][0-9]*")) }) return null
        return parts.map { it.toLongOrNull() ?: return null }
    }
    val remoteParts = parse(remote) ?: return false
    val currentParts = parse(current) ?: return false
    for (i in 0..2) {
        if (remoteParts[i] != currentParts[i]) return remoteParts[i] > currentParts[i]
    }
    return false
}

internal fun matchingApk(assets: List<Pair<String, String>>, supportedAbis: List<String>): String? {
    for (abi in supportedAbis) {
        assets.firstOrNull { (name, url) ->
            name.startsWith("RustEpubReader-Android-$abi-") && name.endsWith(".apk") && url.isNotBlank()
        }?.let { return it.second }
    }
    return null
}
