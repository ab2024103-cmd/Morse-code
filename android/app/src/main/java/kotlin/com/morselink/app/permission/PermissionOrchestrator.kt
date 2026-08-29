package com.morselink.app.permission

import android.Manifest
import android.os.Build
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.morselink.app.R

/**
 * Branches permission requests by API level, exactly as specified by the
 * project brief:
 *
 *   API 23-30 → ACCESS_FINE_LOCATION (unlocks Wi-Fi / BLE scan results)
 *   API 31+   → BLUETOOTH_SCAN, BLUETOOTH_CONNECT, NEARBY_WIFI_DEVICES
 *   API 33+   → additionally POST_NOTIFICATIONS
 *
 * Storage permissions are handled separately when the user first opens the
 * file browser; this only covers discovery + notifications.
 */
class PermissionOrchestrator(private val activity: AppCompatActivity) {

    private lateinit var launcher: ActivityResultLauncher<Array<String>>

    fun runIfNeeded() {
        launcher = activity.registerForActivityResult(
            ActivityResultContracts.RequestMultiplePermissions()
        ) { }

        if (isGranted()) return

        // One-time explainer with plain-language rationale.
        MaterialAlertDialogBuilder(activity)
            .setTitle(R.string.perm_location_title)
            .setMessage(R.string.perm_location_body)
            .setPositiveButton(R.string.allow) { _, _ -> request() }
            .setNegativeButton(R.string.deny, null)
            .setCancelable(false)
            .show()
    }

    private fun request() {
        val perms = mutableListOf<String>()
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.R) {
            // API 23-30: location unlocks Wi-Fi/BLE scan.
            perms += Manifest.permission.ACCESS_FINE_LOCATION
            perms += Manifest.permission.ACCESS_COARSE_LOCATION
        } else {
            // API 31+.
            perms += Manifest.permission.BLUETOOTH_SCAN
            perms += Manifest.permission.BLUETOOTH_CONNECT
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                perms += Manifest.permission.NEARBY_WIFI_DEVICES
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            perms += Manifest.permission.POST_NOTIFICATIONS
        }
        launcher.launch(perms.toTypedArray())
    }

    private fun isGranted(): Boolean {
        // We only consider the request "done" if the primary discovery perm is
        // granted; the rest are additive.
        return when {
            Build.VERSION.SDK_INT <= Build.VERSION_CODES.R ->
                activity.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) ==
                    android.content.pm.PackageManager.PERMISSION_GRANTED
            else ->
                activity.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN) ==
                    android.content.pm.PackageManager.PERMISSION_GRANTED
        }
    }
}
