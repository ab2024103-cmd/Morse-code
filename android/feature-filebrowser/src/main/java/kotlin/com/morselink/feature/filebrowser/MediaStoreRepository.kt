package com.morselink.feature.filebrowser

import android.content.ContentUris
import android.content.Context
import android.provider.MediaStore
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

/**
 * Queries [MediaStore] for the user's media through `ContentResolver` only —
 * never raw `File` paths, which future-proofs against scoped storage across
 * API 23 → latest. Results are grouped by `DATE_ADDED` into headers.
 */
class MediaStoreRepository(private val context: Context) {

    /** A row is either a date header or one media item. */
    sealed class Row {
        data class Header(val label: String, val groupEpochDay: Long) : Row()
        data class Item(val item: MediaFileItem) : Row()
    }

    fun load(kind: MediaKind): List<Row> {
        val items = queryMedia(kind)
        return groupByDate(items)
    }

    private fun queryMedia(kind: MediaKind): List<MediaFileItem> {
        val collection = MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL)
        val projection = arrayOf(
            MediaStore.Files.FileColumns._ID,
            MediaStore.Files.FileColumns.DISPLAY_NAME,
            MediaStore.Files.FileColumns.MIME_TYPE,
            MediaStore.Files.FileColumns.SIZE,
            MediaStore.Files.FileColumns.DATE_ADDED,
            MediaStore.Files.FileColumns.MEDIA_TYPE
        )

        val selection = when (kind) {
            MediaKind.PHOTOS ->
                "${MediaStore.Files.FileColumns.MEDIA_TYPE} = ?"
            MediaKind.VIDEOS ->
                "${MediaStore.Files.FileColumns.MEDIA_TYPE} = ?"
            MediaKind.MUSIC ->
                "${MediaStore.Files.FileColumns.MEDIA_TYPE} = ?"
            MediaKind.DOCS ->
                "${MediaStore.Files.FileColumns.MEDIA_TYPE} = ? " +
                    "OR " +
                    "${MediaStore.Files.FileColumns.MIME_TYPE} LIKE ? OR " +
                    "${MediaStore.Files.FileColumns.MIME_TYPE} LIKE ?"
            MediaKind.ALL -> null
        }
        val args = when (kind) {
            MediaKind.PHOTOS -> arrayOf(MediaStore.Files.FileColumns.MEDIA_TYPE_IMAGE.toString())
            MediaKind.VIDEOS -> arrayOf(MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO.toString())
            MediaKind.MUSIC -> arrayOf(MediaStore.Files.FileColumns.MEDIA_TYPE_AUDIO.toString())
            MediaKind.DOCS -> arrayOf(
                MediaStore.Files.FileColumns.MEDIA_TYPE_DOCUMENT.toString(),
                "%application/pdf%",
                "%wordprocessingml%"
            )
            MediaKind.ALL -> null
        }

        val sort = "${MediaStore.Files.FileColumns.DATE_ADDED} DESC"

        val result = mutableListOf<MediaFileItem>()
        context.contentResolver.query(collection, projection, selection, args, sort)?.use { cursor ->
            val idCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns._ID)
            val nameCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.DISPLAY_NAME)
            val mimeCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.MIME_TYPE)
            val sizeCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.SIZE)
            val dateCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.DATE_ADDED)
            while (cursor.moveToNext()) {
                val id = cursor.getLong(idCol)
                val uri = ContentUris.withAppendedId(collection, id)
                result += MediaFileItem(
                    id = id,
                    uri = uri,
                    displayName = cursor.getString(nameCol) ?: "Unknown",
                    mimeType = cursor.getString(mimeCol) ?: "application/octet-stream",
                    size = cursor.getLong(sizeCol),
                    dateAdded = cursor.getLong(dateCol)
                )
            }
        }
        return result
    }

    private fun groupByDate(items: List<MediaFileItem>): List<Row> {
        val rows = mutableListOf<Row>()
        var currentEpochDay = Long.MIN_VALUE
        for (item in items) {
            val epochDay = epochDayOf(item.dateAdded * 1000L)
            if (epochDay != currentEpochDay) {
                currentEpochDay = epochDay
                rows += Row.Header(formatDate(epochDay), epochDay)
            }
            rows += Row.Item(item)
        }
        return rows
    }

    companion object {
        private fun epochDayOf(epochMillis: Long): Long = epochMillis / 86_400_000L

        private fun formatDate(epochDay: Long): String {
            val cal = Calendar.getInstance().apply {
                timeInMillis = epochDay * 86_400_000L
            }
            val today = Calendar.getInstance()
            if (isSameDay(cal, today)) return "Today"
            val yesterday = Calendar.getInstance().apply { add(Calendar.DAY_OF_YEAR, -1) }
            if (isSameDay(cal, yesterday)) return "Yesterday"
            return SimpleDateFormat("MMM d, yyyy", Locale.getDefault()).format(Date(cal.timeInMillis))
        }

        private fun isSameDay(a: Calendar, b: Calendar): Boolean =
            a.get(Calendar.YEAR) == b.get(Calendar.YEAR) &&
                a.get(Calendar.DAY_OF_YEAR) == b.get(Calendar.DAY_OF_YEAR)
    }
}
