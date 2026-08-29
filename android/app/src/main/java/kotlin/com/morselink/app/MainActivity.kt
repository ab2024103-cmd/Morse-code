package com.morselink.app

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.morselink.app.permission.PermissionOrchestrator
import com.morselink.app.transfer.TransferController
import com.morselink.feature.filebrowser.MediaBrowserFragment
import com.morselink.feature.filebrowser.MediaFileItem
import com.morselink.feature.filebrowser.MediaKind

/**
 * Navigation shell. Classic Views architecture (no Compose) for low-end
 * hardware. Hosts one feature fragment per bottom-nav tab and drives the
 * permission orchestration on first launch.
 */
class MainActivity : AppCompatActivity() {

    companion object {
        private const val STATE_TAB = "state_tab"
    }

    private lateinit var nav: BottomNavigationView
    private val permissionOrchestrator by lazy { PermissionOrchestrator(this) }

    private val tabs: Map<Int, () -> Fragment> = mapOf(
        R.id.tab_photos to { MediaBrowserFragment.newInstance(MediaKind.PHOTOS) },
        R.id.tab_videos to { MediaBrowserFragment.newInstance(MediaKind.VIDEOS) },
        R.id.tab_music to { MediaBrowserFragment.newInstance(MediaKind.MUSIC) },
        R.id.tab_docs to { MediaBrowserFragment.newInstance(MediaKind.DOCS) },
        R.id.tab_files to { MediaBrowserFragment.newInstance(MediaKind.ALL) }
    )

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        nav = findViewById(R.id.bottom_nav)
        nav.setOnItemSelectedListener { item ->
            switchTo(item.itemId)
            true
        }

        if (savedInstanceState == null) {
            nav.selectedItemId = R.id.tab_photos
            permissionOrchestrator.runIfNeeded()
        } else {
            val restored = savedInstanceState.getInt(STATE_TAB, R.id.tab_photos)
            nav.selectedItemId = restored
        }
    }

    private fun switchTo(tab: Int) {
        val fragment = tabs[tab]?.invoke() ?: return
        if (fragment is MediaBrowserFragment) {
            fragment.setOpenHost { item -> MediaLauncher.open(this, item) }
            fragment.setSendHost { items -> TransferController.start(this, items.map { it.uri }) }
        }
        supportFragmentManager.beginTransaction()
            .replace(R.id.fragment_host, fragment, "tab_$tab")
            .commitAllowingStateLoss()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putInt(STATE_TAB, nav.selectedItemId)
        super.onSaveInstanceState(outState)
    }
}
