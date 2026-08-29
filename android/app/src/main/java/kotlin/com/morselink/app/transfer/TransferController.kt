package com.morselink.app.transfer

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build

/**
 * Starts a transfer of selected items to a discovered peer. For this reference
 * implementation the destination is resolved by the discovery layer; the actual
 * send is delegated to the engine via the foreground service. `content://`
 * sources are streamed through the engine's file chunking (a resolver-backed
 * InputStream is opened by the native layer per URI).
 */
object TransferController {

    fun start(context: Context, uris: List<Uri>) {
        if (uris.isEmpty()) return
        val intent = Intent(context, TransferForegroundService::class.java)
            .putExtra(TransferForegroundService.EXTRA_ACTION, TransferForegroundService.ACTION_START)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent)
        } else {
            context.startService(intent)
        }
        // TODO(enqueue): pass `uris` to the engine once a peer is selected via
        // discovery. The native core accepts a list of content:// URIs.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val _ = uris
        }
    }
}
