package com.morselink.app

import android.content.Context
import android.content.Intent
import com.morselink.feature.docviewer.DocumentViewerActivity
import com.morselink.feature.filebrowser.MediaFileItem
import com.morselink.feature.imageviewer.ImageViewerActivity
import com.morselink.feature.musicplayer.MusicPlayerActivity
import com.morselink.feature.videoplayer.VideoPlayerActivity

/**
 * Routes a tapped media item to the matching viewer module. The media viewers
 * are pure platform-native modules (Coil / Media3 / PdfRenderer) with zero
 * dependency on the Rust core — they only read finished files, per the
 * architecture rule.
 */
object MediaLauncher {
    fun open(context: Context, item: MediaFileItem) {
        val uri = item.uri
        val intent: Intent? = when {
            item.isImage() -> ImageViewerActivity.newIntent(context, uri)
            item.isVideo() -> VideoPlayerActivity.newIntent(context, uri)
            item.isAudio() -> MusicPlayerActivity.newIntent(context, uri)
            item.isDoc() -> DocumentViewerActivity.newIntent(context, uri)
            else -> null
        }
        intent?.let {
            it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            context.startActivity(it)
        }
    }
}
