package com.morselink.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build

/**
 * Application root. Owns the singleton [MorseLinkEngine] so the UI and the
 * foreground transfer service share one QUIC session, and creates the
 * notification channel guarded behind the API 26+ check (minSdk 23).
 */
class MorseLinkApp : Application() {

    companion object {
        const val CHANNEL_TRANSFER = "morselink.transfer"
        lateinit var instance: MorseLinkApp
            private set
    }

    val engine: MorseLinkEngine by lazy { MorseLinkEngine(this) }

    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannels()
    }

    private fun createNotificationChannels() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val nm = getSystemService(NotificationManager::class.java)
            val channel = NotificationChannel(
                CHANNEL_TRANSFER,
                getString(R.string.service_channel_transfer),
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = getString(R.string.service_channel_desc)
            }
            nm.createNotificationChannel(channel)
        }
    }
}
