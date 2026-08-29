package com.morselink.feature.videoplayer

import android.app.PictureInPictureParams
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Rational
import androidx.appcompat.app.AppCompatActivity
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView

/**
 * Hardware-decoded video player built on Media3 (ExoPlayer). Supports
 * picture-in-picture (a differentiator) and subtitle rendering (SRT/VTT),
 * both native to Media3. Thumbnails in list rows are cached separately.
 */
class VideoPlayerActivity : AppCompatActivity(R.layout.activity_video_player) {

    private var player: ExoPlayer? = null
    private var playerView: PlayerView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent.getParcelableExtra<Uri>(EXTRA_URI)
        playerView = findViewById(R.id.player_view)

        val p = ExoPlayer.Builder(this).build()
        p.setMediaItem(MediaItem.fromUri(uri))
        p.playWhenReady = true
        p.prepare()
        p.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                updatePipParams()
            }
        })
        playerView?.player = p
        player = p
    }

    private fun updatePipParams() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        if (!packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return
        val aspect = Rational(16, 9)
        setPictureInPictureParams(
            PictureInPictureParams.Builder()
                .setAspectRatio(aspect)
                .build()
        )
    }

    override fun onUserLeaveHint() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            enterPictureInPictureMode(PictureInPictureParams.Builder().build())
        }
        super.onUserLeaveHint()
    }

    override fun onStop() {
        super.onStop()
        if (isInPictureInPictureMode) {
            // Keep playing in PiP.
        } else {
            player?.pause()
        }
    }

    override fun onDestroy() {
        playerView?.player = null
        player?.release()
        player = null
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_URI = "com.morselink.feature.videoplayer.EXTRA_URI"

        fun newIntent(context: Context, uri: Uri): Intent =
            Intent(context, VideoPlayerActivity::class.java).apply {
                putExtra(EXTRA_URI, uri)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
    }
}
