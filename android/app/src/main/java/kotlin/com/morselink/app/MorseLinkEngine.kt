package com.morselink.app

import android.content.Context
import morselink_core.EngineConfig
import morselink_core.EngineError
import morselink_core.TransferEngine
import morselink_core.TransferObserver

/**
 * Thin wrapper around the UniFFI-generated [TransferEngine]. The Kotlin
 * bindings are emitted by the in-repo `uniffi-bindgen` CLI from the shared Rust
 * core (see :core-transfer/build.gradle) into package `morselink_core`
 * (configured via `rust-core/uniffi.toml`), so this is the only place the app
 * references the generated class names.
 *
 * Note the UniFFI Kotlin primitive mapping used here:
 *   `u16` -> `UShort`, `u64` -> `ULong`, `f64` -> `Double`, `bool` -> `Boolean`.
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
        // UniFFI 0.28 maps the Rust `#[uniffi::constructor] fn new(...)` to the
        // object's primary Kotlin constructor, so instantiate directly (not via a
        // `.new(...)` companion). The constructor throws `EngineError` (a sealed
        // Exception subclass) on failure.
        val e = try {
            TransferEngine(config)
        } catch (err: EngineError) {
            throw IllegalStateException("Could not create transfer engine: ${err.message}", err)
        }
        e.setObserver(object : TransferObserver {
            override fun onProgress(streamId: ULong, bytesDone: ULong, bytesTotal: ULong) {
                // Progress can be dispatched here; the UI subscribes via a bus.
            }
            override fun onPeerDiscovered(peerId: String, peerName: String, addr: String) {
            }
            override fun onTransferComplete(fileName: String, total: ULong) {
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
