package com.morselink.feature.musicplayer

import android.content.Intent
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/**
 * Background music playback via Media3 [MediaSessionService]. Provides
 * lock-screen / notification controls and keeps playing across tabs (the
 * persistent mini-player is rendered by the app shell bound to this session).
 */
class MusicPlayerService : MediaSessionService() {

    private var player: ExoPlayer? = null
    private var mediaSession: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        val p = ExoPlayer.Builder(this).build()
        player = p
        mediaSession = MediaSession.Builder(this, p).build()
    }

    fun play(uri: String, title: String) {
        val p = player ?: return
        val item = MediaItem.Builder()
            .setUri(uri)
            .setMediaMetadata(
                MediaMetadata.Builder().setTitle(title).build()
            )
            .build()
        p.setMediaItem(item)
        p.prepare()
        p.play()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val uri = intent?.getStringExtra("uri")
        val title = intent?.getStringExtra("title")
        if (uri != null) {
            // MediaSessionService manages the foreground notification based on
            // play state; we just start playback.
            play(uri, title ?: "Track")
        }
        return super.onStartCommand(intent, flags, startId)
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? =
        mediaSession

    override fun onDestroy() {
        mediaSession?.release()
        player?.release()
        mediaSession = null
        player = null
        super.onDestroy()
    }
}
