package com.morselink.feature.musicplayer

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView

/**
 * Simple now-playing screen. Playback runs in [MusicPlayerService] so it
 * continues in the background / on the lock screen even if this Activity is
 * left; the app shell hosts a persistent mini-player bound to the same session.
 */
class MusicPlayerActivity : AppCompatActivity(R.layout.activity_music_player) {

    private var player: ExoPlayer? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent.getStringExtra(EXTRA_URI)
        val title = intent.getStringExtra(EXTRA_TITLE) ?: "Track"

        val p = ExoPlayer.Builder(this).build()
        p.setMediaItem(
            MediaItem.Builder()
                .setUri(uri)
                .build()
        )
        p.playWhenReady = true
        p.prepare()
        findViewById<PlayerView>(R.id.player_view).player = p
        player = p

        // Kick off the background session service for lock-screen controls.
        val svc = Intent(this, MusicPlayerService::class.java)
        svc.putExtra("uri", uri)
        svc.putExtra("title", title)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(svc)
        } else {
            startService(svc)
        }
    }

    override fun onDestroy() {
        findViewById<PlayerView?>(R.id.player_view)?.player = null
        player?.release()
        player = null
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_URI = "com.morselink.feature.musicplayer.EXTRA_URI"
        private const val EXTRA_TITLE = "com.morselink.feature.musicplayer.EXTRA_TITLE"

        fun newIntent(context: Context, uri: Uri): Intent =
            Intent(context, MusicPlayerActivity::class.java).apply {
                putExtra(EXTRA_URI, uri.toString())
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
    }
}
