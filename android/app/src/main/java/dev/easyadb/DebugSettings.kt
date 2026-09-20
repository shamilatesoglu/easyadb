package dev.easyadb

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.provider.Settings
import android.widget.Toast

object DebugSettings {
    fun intent() = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        // AOSP highlights the wireless debugging row. OEMs may ignore this hint.
        putExtra(":settings:fragment_args_key", "toggle_adb_wireless")
    }

    fun open(activity: Activity) {
        try {
            if (Settings.Global.getInt(activity.contentResolver, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0) == 0) {
                activity.startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
                Toast.makeText(activity, R.string.developer_detail, Toast.LENGTH_LONG).show()
                return
            }
            activity.startActivity(intent())
            Toast.makeText(activity, R.string.settings_hint, Toast.LENGTH_LONG).show()
        } catch (_: ActivityNotFoundException) {
            fallback(activity)
        } catch (_: SecurityException) {
            fallback(activity)
        }
    }

    private fun fallback(activity: Activity) {
        try {
            activity.startActivity(Intent(Settings.ACTION_SETTINGS))
        } catch (_: ActivityNotFoundException) {
            // A restricted profile may have no available Settings activity.
        } catch (_: SecurityException) {
            // Some managed profiles prohibit opening Settings.
        }
        Toast.makeText(activity, R.string.settings_unavailable, Toast.LENGTH_LONG).show()
    }
}
