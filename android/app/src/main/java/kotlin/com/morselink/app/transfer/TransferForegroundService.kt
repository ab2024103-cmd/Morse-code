package com.morselink.app.transfer

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.morselink.app.MainActivity
import com.morselink.app.MorseLinkApp
import com.morselink.app.R

/**
 * Runs the QUIC transfer engine as a foreground service with an explicit
 * `dataSync` type (declared in the manifest). Keeps transfers alive while the
 * user is in another app or the screen is off.
 */
class TransferForegroundService : Service() {

    companion object {
        private const val CHANNEL_ID = MorseLinkApp.CHANNEL_TRANSFER
        private const val NOTIFICATION_ID = 1001
        const val EXTRA_ACTION = "action"
        const val ACTION_START = "start"
        const val ACTION_STOP = "stop"
    }

    override fun onCreate() {
        super.onCreate()
        MorseLinkApp.instance.engine.start()
        startForegroundCompat()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.getStringExtra(EXTRA_ACTION) ?: ACTION_START
        if (action == ACTION_STOP) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
        return START_STICKY
    }

    override fun onDestroy() {
        MorseLinkApp.instance.engine.shutdown()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun startForegroundCompat() {
        val contentIntent = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val notification: Notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.transfer_ongoing))
            .setContentText("QUIC / TLS 1.3")
            .setSmallIcon(R.drawable.ic_tab_file)
            .setOngoing(true)
            .setContentIntent(contentIntent)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification,
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }
}
