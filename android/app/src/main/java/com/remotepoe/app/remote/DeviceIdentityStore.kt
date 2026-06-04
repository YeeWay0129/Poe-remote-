package com.remotepoe.app.remote

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyPairGenerator
import java.security.KeyStore
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

        val publicKey = keyStore.getCertificate(KEY_ALIAS).publicKey.encoded
        return Base64.encodeToString(publicKey, Base64.NO_WRAP)
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
    }
}
