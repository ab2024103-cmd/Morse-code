package com.morselink.feature.filebrowser

import android.os.Bundle
import android.view.View
import androidx.core.os.bundleOf
import androidx.fragment.app.Fragment
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.selection.SelectionPredicates
import androidx.recyclerview.selection.SelectionTracker
import androidx.recyclerview.selection.StorageStrategy
import androidx.recyclerview.widget.LinearLayoutManager
import com.morselink.feature.filebrowser.databinding.FragmentMediaBrowserBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Host integration points, implemented by the app module. */
fun interface MediaOpenHost {
    fun open(item: MediaFileItem)
}

fun interface MediaSendHost {
    fun send(items: List<MediaFileItem>)
}

/**
 * MediaStore-backed categorized listing with date-header rows and a
 * [SelectionTracker]-driven selection model (long-press to enter selection
 * mode; header checkbox bulk-selects a whole date group).
 */
class MediaBrowserFragment : Fragment(R.layout.fragment_media_browser) {

    private var _binding: FragmentMediaBrowserBinding? = null
    private val binding get() = _binding!!

    private lateinit var kind: MediaKind
    private lateinit var repository: MediaStoreRepository
    private var adapter: MediaFileAdapter? = null
    private var tracker: SelectionTracker<String>? = null

    private var openHost: MediaOpenHost = MediaOpenHost { }
    private var sendHost: MediaSendHost = MediaSendHost { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        kind = MediaKind.valueOf(requireArguments().getString(ARG_KIND, "PHOTOS"))
        repository = MediaStoreRepository(requireContext())
    }

    fun setOpenHost(host: MediaOpenHost) { openHost = host }
    fun setSendHost(host: MediaSendHost) { sendHost = host }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        _binding = FragmentMediaBrowserBinding.bind(view)

        binding.recycler.layoutManager = LinearLayoutManager(requireContext())

        viewLifecycleOwner.lifecycleScope.launch {
            val rows = withContext(Dispatchers.IO) { repository.load(kind) }
            if (rows.isEmpty()) {
                binding.recycler.visibility = View.GONE
                binding.sendFab.visibility = View.GONE
                return@launch
            }
            binding.recycler.visibility = View.VISIBLE
            val newAdapter = MediaFileAdapter(
                rows,
                onOpenItem = { openHost.open(it) },
                onToggleItem = { item, selected -> toggleItem(item, selected) },
                onToggleHeader = { toggleHeader(it) }
            )
            adapter = newAdapter
            binding.recycler.adapter = newAdapter

            val tracker = SelectionTracker.Builder(
                "media_selection_$kind",
                binding.recycler,
                MediaKeyProvider(newAdapter),
                MediaItemDetailsLookup(binding.recycler),
                StorageStrategy.createStringStorage()
            )
                .withPredicates(SelectionPredicates.Builder<String>().build())
                .build()
            newAdapter.attachSelection(tracker)
            this@MediaBrowserFragment.tracker = tracker

            tracker.addObserver(object : SelectionTracker.SelectionObserver<String>() {
                override fun onSelectionChanged() {
                    val count = tracker.selection.size()
                    binding.sendFab.visibility = if (count > 0) View.VISIBLE else View.GONE
                    binding.sendFab.text =
                        requireContext().getString(R.string.send_selected, count)
                }
            })

            binding.sendFab.setOnClickListener {
                val selected = tracker.selection.asIterable().toList()
                val items = rows
                    .filterIsInstance<MediaStoreRepository.Row.Item>()
                    .map { it.item }
                    .filter { selected.contains(it.stableKey) }
                sendHost.send(items)
            }
        }
    }

    private fun toggleItem(item: MediaFileItem, selected: Boolean) {
        if (selected) tracker?.select(item.stableKey) else tracker?.deselect(item.stableKey)
    }

    private fun toggleHeader(header: MediaStoreRepository.Row.Header) {
        val rowList = adapter?.rows ?: return
        val items = rowList
            .filterIsInstance<MediaStoreRepository.Row.Item>()
            .filter { it.item.dateAdded / 86_400_000L == header.groupEpochDay }
        val allSelected = items.all { tracker?.isSelected(it.item.stableKey) == true }
        items.forEach { item ->
            if (allSelected) tracker?.deselect(item.item.stableKey)
            else tracker?.select(item.item.stableKey)
        }
    }

    override fun onDestroyView() {
        _binding = null
        adapter = null
        super.onDestroyView()
    }

    companion object {
        private const val ARG_KIND = "arg_kind"

        fun newInstance(kind: MediaKind): MediaBrowserFragment =
            MediaBrowserFragment().apply {
                arguments = bundleOf(ARG_KIND to kind.name)
            }
    }
}
