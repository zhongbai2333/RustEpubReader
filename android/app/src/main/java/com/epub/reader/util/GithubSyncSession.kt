package com.zhongbai233.epub.reader.util

/** A result may apply only to the still-enabled foreground session that requested it. */
fun canApplyGithubSync(requestGeneration: Long, currentGeneration: Long, requestOwner: String,
    currentOwner: String?, enabled: Boolean, foreground: Boolean): Boolean =
    requestGeneration == currentGeneration && requestOwner == currentOwner && enabled && foreground

/** Only full repo authorizes private repository synchronization. */
fun hasGithubRepoScope(scopes: String): Boolean = scopes.split(',').any { it.trim() == "repo" }
