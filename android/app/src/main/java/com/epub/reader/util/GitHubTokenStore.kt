package com.zhongbai233.epub.reader.util

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** The encryption key stays in Android Keystore. Token ciphertext stays outside backups. */
object GitHubTokenStore {
    private const val ALIAS = "epub_reader_github_token"
    private const val TOKEN = "github_token_encrypted"
    private fun preferences(context: Context) = context.getSharedPreferences("github_credentials", Context.MODE_PRIVATE)
    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256).build())
        return generator.generateKey()
    }
    fun save(context: Context, token: String): Result<Unit> = runCatching {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val encoded = Base64.encodeToString(cipher.iv + cipher.doFinal(token.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
        check(preferences(context).edit().putString(TOKEN, encoded).commit()) { "Cannot save encrypted GitHub token" }
        check(context.getSharedPreferences("reader_settings", Context.MODE_PRIVATE).edit().remove("github_token").commit()) { "Cannot remove legacy GitHub token" }
    }
    fun load(context: Context): String? {
        val encoded = preferences(context).getString(TOKEN, null)
        if (encoded != null) return runCatching {
            val bytes = Base64.decode(encoded, Base64.NO_WRAP)
            require(bytes.size > 28)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(0,12)))
            String(cipher.doFinal(bytes.copyOfRange(12, bytes.size)), Charsets.UTF_8)
        }.getOrNull()
        val legacy = context.getSharedPreferences("reader_settings", Context.MODE_PRIVATE).getString("github_token", null) ?: return null
        return if (save(context, legacy).isSuccess) legacy else null
    }
    fun delete(context: Context) {
        preferences(context).edit().remove(TOKEN).commit()
        context.getSharedPreferences("reader_settings", Context.MODE_PRIVATE).edit().remove("github_token").commit()
    }
}
