package com.morselink.feature.filebrowser

import android.view.LayoutInflater
import android.view.MotionEvent
import android.view.ViewGroup
import androidx.recyclerview.selection.ItemDetailsLookup
import androidx.recyclerview.selection.ItemKeyProvider
import androidx.recyclerview.selection.SelectionTracker
import androidx.recyclerview.widget.RecyclerView
import coil.load
import com.morselink.feature.filebrowser.databinding.ItemMediaBinding
import com.morselink.feature.filebrowser.databinding.ItemMediaHeaderBinding

/**
 * Flat RecyclerView list with two ViewHolder types: a date header and a media
 * item. Selection is entirely driven by a [SelectionTracker] — the adapter only
 * reads `tracker.isSelected(key)`, never stores its own selection set.
 */
class MediaFileAdapter(
    val rows: List<MediaStoreRepository.Row>,
    private val onOpenItem: (MediaFileItem) -> Unit,
    private val onToggleItem: (MediaFileItem, Boolean) -> Unit,
    private val onToggleHeader: (MediaStoreRepository.Row.Header) -> Unit
) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    private var tracker: SelectionTracker<String>? = null

    init {
        setHasStableIds(true)
    }

    fun attachSelection(tracker: SelectionTracker<String>) {
        this.tracker = tracker
        tracker.addObserver(object : SelectionTracker.SelectionObserver<String>() {
            override fun onSelectionChanged() {
                notifyDataSetChanged()
            }
        })
    }

    fun keyAt(position: Int): String? =
        (rows[position] as? MediaStoreRepository.Row.Item)?.item?.stableKey

    fun positionOfKey(key: String): Int =
        rows.indexOfFirst { it is MediaStoreRepository.Row.Item && it.item.stableKey == key }

    override fun getItemViewType(position: Int): Int = when (rows[position]) {
        is MediaStoreRepository.Row.Header -> TYPE_HEADER
        is MediaStoreRepository.Row.Item -> TYPE_ITEM
    }

    override fun getItemId(position: Int): Long = when (val r = rows[position]) {
        is MediaStoreRepository.Row.Header -> -r.groupEpochDay
        is MediaStoreRepository.Row.Item -> r.item.id
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            TYPE_HEADER -> HeaderViewHolder(ItemMediaHeaderBinding.inflate(inflater, parent, false))
            else -> ItemViewHolder(ItemMediaBinding.inflate(inflater, parent, false))
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when (val r = rows[position]) {
            is MediaStoreRepository.Row.Header -> (holder as HeaderViewHolder).bind(r)
            is MediaStoreRepository.Row.Item -> (holder as ItemViewHolder).bind(r.item)
        }
    }

    override fun getItemCount(): Int = rows.size

    private inner class HeaderViewHolder(private val b: ItemMediaHeaderBinding) :
        RecyclerView.ViewHolder(b.root) {
        fun bind(header: MediaStoreRepository.Row.Header) {
            b.headerTitle.text = header.label
            b.headerCheck.setOnClickListener { onToggleHeader(header) }
            b.root.setOnClickListener { onToggleHeader(header) }
        }
    }

    private inner class ItemViewHolder(private val b: ItemMediaBinding) :
        RecyclerView.ViewHolder(b.root) {
        fun bind(item: MediaFileItem) {
            val key = item.stableKey
            val selected = tracker?.isSelected(key) ?: false
            b.itemCheck.isChecked = selected
            b.itemTitle.text = item.displayName
            b.itemMeta.text = "${formatSize(item.size)} · ${item.mimeType}"
            // Coil loads thumbnails straight from the ContentResolver URI —
            // no full-res decode for list rows.
            b.itemThumb.load(item.uri) {
                crossfade(true)
                placeholder(R.drawable.ic_placeholder_thumb)
            }
            b.itemThumb.alpha = if (selected) 0.55f else 1f
            b.itemCheck.setOnClickListener { onToggleItem(item, !selected) }
            b.root.setOnClickListener {
                if (selected) onToggleItem(item, false) else onOpenItem(item)
            }
        }
    }

    companion object {
        const val TYPE_HEADER = 0
        const val TYPE_ITEM = 1

        fun formatSize(bytes: Long): String = when {
            bytes < 1024 -> "$bytes B"
            bytes < 1024 * 1024 -> "%.1f KB".format(bytes / 1024.0)
            bytes < 1024 * 1024 * 1024 -> "%.1f MB".format(bytes / (1024.0 * 1024))
            else -> "%.1f GB".format(bytes / (1024.0 * 1024 * 1024))
        }
    }
}

/** Selection key provider (recyclerview-selection). */
class MediaKeyProvider(private val adapter: MediaFileAdapter) :
    ItemKeyProvider<String>(SCOPE_CACHED) {
    override fun getKey(position: Int): String? = adapter.keyAt(position)
    override fun getPosition(key: String): Int = adapter.positionOfKey(key)
}

/** Details lookup supporting the two-row layout. */
class MediaItemDetailsLookup(private val recyclerView: RecyclerView) :
    ItemDetailsLookup<String>() {

    override fun getItemDetails(motionEvent: MotionEvent): ItemDetails<String>? {
        val itemView =
            recyclerView.findChildViewUnder(motionEvent.x, motionEvent.y) ?: return null
        val position = recyclerView.getChildAdapterPosition(itemView)
        if (position == RecyclerView.NO_POSITION) return null
        val adapter = recyclerView.adapter as? MediaFileAdapter ?: return null
        return object : ItemDetails<String>() {
            override fun getPosition(): Int = position
            override fun getSelectionKey(): String? {
                // Only items are selectable; headers are handled separately.
                val row = adapter.rows.getOrNull(position)
                return (row as? MediaStoreRepository.Row.Item)?.item?.stableKey
            }
        }
    }
}
