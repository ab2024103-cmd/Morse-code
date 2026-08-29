package com.morselink.feature.filebrowser

import android.net.Uri

/**
 * A single media item surfaced from MediaStore. Identified by [uri] (used for
 * both selection identity and display) — the app never works with raw File
 * paths, per the scoped-storage mandate.
 */
data class MediaFileItem(
    val id: Long,
    val uri: Uri,
    val displayName: String,
    val mimeType: String,
    val size: Long,
    val dateAdded: Long,
    val bucketGrouping: Long = 0L
) {
    /** Stable selection key. */
    val stableKey: String get() = uri.toString()

    fun isImage() = mimeType.startsWith("image/")
    fun isVideo() = mimeType.startsWith("video/")
    fun isAudio() = mimeType.startsWith("audio/")
    fun isDoc() =
        mimeType == "application/pdf" ||
            mimeType.contains("wordprocessingml") ||
            mimeType.contains("spreadsheetml") ||
            mimeType.contains("presentationml")
}

/** A date header row (grouped by DATE_ADDED), rendered between item groups. */
data class DateHeader(
    val epochDay: Long,
    val label: String
)
