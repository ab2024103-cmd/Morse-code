package com.morselink.feature.imageviewer

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import coil.load

/**
 * Fullscreen image viewer. Loads the full-resolution image with Coil from the
 * `content://` URI (ContentResolver) and shows it in a pinch-zoom [ZoomImageView].
 * Supported: JPEG, PNG, WebP, GIF, HEIC/HEIF (via Coil's decoders).
 */
class ImageViewerActivity : AppCompatActivity(R.layout.activity_image_viewer) {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent.getParcelableExtra<Uri>(EXTRA_URI)
        val zoom = findViewById<ZoomImageView>(R.id.zoom_image)
        if (uri != null) {
            zoom.load(uri) { crossfade(true) }
        }
    }

    companion object {
        private const val EXTRA_URI = "com.morselink.feature.imageviewer.EXTRA_URI"

        fun newIntent(context: Context, uri: Uri): Intent =
            Intent(context, ImageViewerActivity::class.java).apply {
                putExtra(EXTRA_URI, uri)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
    }
}
