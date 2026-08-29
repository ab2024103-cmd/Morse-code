package com.morselink.app

import android.content.Context
import morselink_core.EngineConfig
import morselink_core.EngineError
import morselink_core.TransferEngine
import morselink_core.TransferObserver

/**
 * Thin wrapper around the UniFFI-generated [TransferEngine]. The Kotlin
 * bindings are emitted by `uniffi-bindgen` from the shared Rust core (see
 * :core-transfer/build.gradle), so this is the only place the app references
 * the generated class names. If bindgen renames anything, fix it here first.
 */
class MorseLinkEngine(private val context: Context) {

    private var engine: TransferEngine? = null

    fun start() {
        val config = EngineConfig(
            listenAddr = "0.0.0.0",
            port = 0.toUShort(),
            serverName = "morselink.local",
            deviceName = android.os.Build.MODEL ?: "MorseLink device",
            enableDiscovery = true
        )
        val e = TransferEngine.new(config)
        e.setObserver(object : TransferObserver {
            override fun onProgress(streamId: Long, bytesDone: Long, bytesTotal: Long) {
                // Progress can be dispatched here; the UI subscribes via a bus.
            }
            override fun onPeerDiscovered(peerId: String, peerName: String, addr: String) {
            }
            override fun onTransferComplete(fileName: String, total: Long) {
            }
        })
        try {
            e.start()
            engine = e
        } catch (err: EngineError) {
            throw IllegalStateException("Could not start transfer engine: ${err.message}", err)
        }
    }

    fun shutdown() {
        engine?.shutdown()
        engine = null
    }
}
