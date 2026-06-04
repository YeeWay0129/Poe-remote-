package com.remotepoe.app.remote

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.nio.charset.StandardCharsets
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.spec.ECGenParameterSpec
import java.util.UUID

class DeviceIdentityStore(context: Context) {
    private val appContext = context.applicationContext
    private val preferences = appContext.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun loadOrCreate(): DeviceIdentity {
        val existingDeviceId = preferences.getString(KEY_DEVICE_ID, null)
        val existingPublicKey = preferences.getString(KEY_PUBLIC_KEY, null)
        if (!existingDeviceId.isNullOrBlank() && !existingPublicKey.isNullOrBlank()) {
            return DeviceIdentity(
                deviceId = existingDeviceId,
                deviceName = deviceName(),
                publicKey = existingPublicKey,
            )
        }

        val identity = DeviceIdentity(
            deviceId = "android-${UUID.randomUUID()}",
            deviceName = deviceName(),
            publicKey = loadOrCreatePublicKey(),
        )
        preferences.edit()
            .putString(KEY_DEVICE_ID, identity.deviceId)
            .putString(KEY_PUBLIC_KEY, identity.publicKey)
            .apply()

        return identity
    }

    private fun loadOrCreatePublicKey(): String {
        runCatching {
            val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
            if (!keyStore.containsAlias(KEY_ALIAS)) {
                val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE)
                val spec = KeyGenParameterSpec.Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
                )
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .build()
                generator.initialize(spec)
                generator.generateKeyPair()
            }

            keyStore.getCertificate(KEY_ALIAS).publicKey.encoded
        }.getOrNull()?.let { publicKey ->
            return Base64.encodeToString(publicKey, Base64.NO_WRAP)
        }

        return fallbackPublicKey()
    }

    private fun fallbackPublicKey(): String {
        val existingFallback = preferences.getString(KEY_FALLBACK_PUBLIC_KEY, null)
        if (!existingFallback.isNullOrBlank()) return existingFallback

        val seed = "${UUID.randomUUID()}:${deviceName()}"
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(seed.toByteArray(StandardCharsets.UTF_8))
        val fallback = Base64.encodeToString(digest, Base64.NO_WRAP)
        preferences.edit()
            .putString(KEY_FALLBACK_PUBLIC_KEY, fallback)
            .apply()

        return fallback
    }

    private fun deviceName(): String =
        listOf(Build.MANUFACTURER, Build.MODEL)
            .filter { it.isNotBlank() }
            .joinToString(" ")
            .ifBlank { "Android device" }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "remote_poe_device_identity"
        const val PREFERENCES_NAME = "remote_poe_identity"
        const val KEY_DEVICE_ID = "device_id"
        const val KEY_PUBLIC_KEY = "public_key"
        const val KEY_FALLBACK_PUBLIC_KEY = "fallback_public_key"
    }
}
