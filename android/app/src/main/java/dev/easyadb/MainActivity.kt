package dev.easyadb

import android.app.Activity
import android.app.AlertDialog
import android.app.StatusBarManager
import android.content.ComponentName
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.Icon
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

class MainActivity : Activity() {
    private val backgroundColor = Color.rgb(16, 24, 21)
    private val cardColor = Color.rgb(27, 40, 34)
    private val mint = Color.rgb(183, 245, 202)
    private val ink = Color.rgb(240, 246, 241)
    private val muted = Color.rgb(169, 188, 176)
    private lateinit var wifiStatus: TextView
    private lateinit var developerStatus: TextView
    private lateinit var developerDetail: TextView
    private val connectivity by lazy { getSystemService(ConnectivityManager::class.java) }
    private var observingNetwork = false
    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) { refreshSoon() }
        override fun onLost(network: Network) { refreshSoon() }
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) { refreshSoon() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT < 35) {
            @Suppress("DEPRECATION")
            window.setDecorFitsSystemWindows(false)
        }
        val scroll = ScrollView(this).apply {
            setBackgroundColor(backgroundColor)
            isFillViewport = true
            clipToPadding = true
            setOnApplyWindowInsetsListener { view, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        val page = column().apply { setPadding(dp(24), dp(22), dp(24), dp(28)) }
        scroll.addView(page)
        setContentView(scroll)

        val brand = row()
        brand.addView(label(R.string.brand, 25f, mint, true), LinearLayout.LayoutParams(0, -2, 1f))
        brand.addView(label(R.string.version, 9f, muted, true))
        page.addView(brand)
        page.addView(label(R.string.eyebrow, 10f, mint, true).apply { letterSpacing = .16f }, spaced(30))
        page.addView(label(R.string.headline, 38f, ink, true).apply { setLineSpacing(0f, 1.02f) }, spaced(14))
        page.addView(label(R.string.intro, 15f, muted).apply { setLineSpacing(dp(3).toFloat(), 1f) }, spaced(14))

        val check = card()
        check.addView(sectionTitle(R.string.check_title))
        wifiStatus = label(R.string.wifi_missing, 16f, ink, true)
        check.addView(wifiStatus, spaced(18))
        check.addView(label(R.string.wifi_detail, 13f, muted), spaced(5))
        developerStatus = label(R.string.developer_missing, 16f, ink, true)
        developerDetail = label(R.string.developer_detail, 13f, muted)
        check.addView(developerStatus, spaced(18))
        check.addView(developerDetail, spaced(5))
        val settingsButtons = row()
        settingsButtons.addView(button(R.string.wifi_button, false) { open(Settings.ACTION_WIFI_SETTINGS) }, LinearLayout.LayoutParams(0, -2, 1f))
        settingsButtons.addView(button(R.string.about_button, false) { open(Settings.ACTION_DEVICE_INFO_SETTINGS) }, LinearLayout.LayoutParams(0, -2, 1f).apply { marginStart = dp(8) })
        check.addView(settingsButtons, spaced(14))
        page.addView(check, spaced(26))

        page.addView(sectionTitle(R.string.steps_title), spaced(28))
        page.addView(step("01", R.string.step_one_title, R.string.step_one_detail), spaced(18))
        val command = label("$ easyadb", 17f, mint, false).apply {
            typeface = Typeface.MONOSPACE
            background = rounded(cardColor, 12)
            setPadding(dp(18), dp(16), dp(18), dp(16))
            setTextIsSelectable(true)
        }
        page.addView(command, spaced(14))
        page.addView(step("02", R.string.step_two_title, R.string.step_two_detail), spaced(20))
        page.addView(step("03", R.string.step_three_title, R.string.step_three_detail), spaced(20))
        page.addView(button(R.string.open_settings, true) { DebugSettings.open(this) }, spaced(24))
        page.addView(label(R.string.system_scanner_note, 12f, muted).apply { setLineSpacing(dp(2).toFloat(), 1f) }, spaced(12))

        val extras = card()
        extras.addView(sectionTitle(R.string.tools_title))
        extras.addView(button(R.string.add_tile, false) { addTile() }, spaced(14))
        extras.addView(button(R.string.help_button, false) {
            AlertDialog.Builder(this).setTitle(R.string.help_title).setMessage(R.string.help_body)
                .setPositiveButton(R.string.got_it, null).show()
        }, spaced(8))
        page.addView(extras, spaced(26))
        page.addView(label(R.string.footer, 11f, muted).apply { gravity = Gravity.CENTER }, spaced(24))
    }

    override fun onStart() {
        super.onStart()
        try {
            connectivity.registerNetworkCallback(NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(), networkCallback)
            observingNetwork = true
        } catch (_: SecurityException) {
            observingNetwork = false
        }
    }

    override fun onResume() { super.onResume(); refreshStatus() }

    override fun onStop() {
        if (observingNetwork) { connectivity.unregisterNetworkCallback(networkCallback); observingNetwork = false }
        super.onStop()
    }

    private fun refreshSoon() { runOnUiThread { if (!isDestroyed) refreshStatus() } }

    private fun refreshStatus() {
        // Inspect physical Wi-Fi networks too, since a VPN can be the default network.
        @Suppress("DEPRECATION")
        val wifi = connectivity.allNetworks.any { connectivity.getNetworkCapabilities(it)?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true }
        val developer = Settings.Global.getInt(contentResolver, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0) == 1
        wifiStatus.setText(if (wifi) R.string.wifi_ready else R.string.wifi_missing)
        wifiStatus.setTextColor(if (wifi) mint else ink)
        developerStatus.setText(if (developer) R.string.developer_ready else R.string.developer_missing)
        developerStatus.setTextColor(if (developer) mint else ink)
        developerDetail.setText(if (developer) R.string.developer_ready_detail else R.string.developer_detail)
    }

    private fun addTile() {
        if (Build.VERSION.SDK_INT >= 33) {
            getSystemService(StatusBarManager::class.java).requestAddTileService(
                ComponentName(this, DebugTileService::class.java), getString(R.string.app_name),
                Icon.createWithResource(this, R.drawable.ic_tile), mainExecutor
            ) { result ->
                val added = result == StatusBarManager.TILE_ADD_REQUEST_RESULT_TILE_ADDED || result == StatusBarManager.TILE_ADD_REQUEST_RESULT_TILE_ALREADY_ADDED
                Toast.makeText(this, if (added) R.string.tile_added else R.string.tile_manual, Toast.LENGTH_LONG).show()
            }
        } else {
            Toast.makeText(this, R.string.tile_manual, Toast.LENGTH_LONG).show()
        }
    }

    private fun open(action: String) {
        try { startActivity(Intent(action)) }
        catch (_: android.content.ActivityNotFoundException) { Toast.makeText(this, R.string.settings_unavailable, Toast.LENGTH_LONG).show() }
        catch (_: SecurityException) { Toast.makeText(this, R.string.settings_unavailable, Toast.LENGTH_LONG).show() }
    }

    private fun step(number: String, title: Int, description: Int): View {
        val row = row().apply { gravity = Gravity.TOP }
        row.addView(label(number, 12f, mint, true).apply {
            gravity = Gravity.CENTER
            background = rounded(cardColor, 12)
        }, LinearLayout.LayoutParams(dp(40), dp(40)))
        val copy = column()
        copy.addView(label(title, 16f, ink, true))
        copy.addView(label(description, 13f, muted).apply { setLineSpacing(dp(2).toFloat(), 1f) }, spaced(6))
        row.addView(copy, LinearLayout.LayoutParams(0, -2, 1f).apply { marginStart = dp(14) })
        return row
    }

    private fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    private fun row() = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
    private fun card() = column().apply { background = rounded(cardColor, 20); setPadding(dp(20), dp(20), dp(20), dp(20)) }
    private fun sectionTitle(id: Int) = label(id, 10f, muted, true).apply { letterSpacing = .12f }
    private fun label(id: Int, size: Float, color: Int, bold: Boolean = false) = label(getString(id), size, color, bold)
    private fun label(value: String, size: Float, color: Int, bold: Boolean) = TextView(this).apply {
        text = value
        textSize = size
        setTextColor(color)
        typeface = Typeface.create("sans-serif", if (bold) Typeface.BOLD else Typeface.NORMAL)
        includeFontPadding = false
    }
    private fun button(id: Int, primary: Boolean, action: () -> Unit) = Button(this).apply {
        setText(id)
        isAllCaps = false
        textSize = if (primary) 16f else 12f
        setTextColor(if (primary) backgroundColor else mint)
        backgroundTintList = ColorStateList.valueOf(if (primary) mint else Color.rgb(37, 55, 45))
        minHeight = dp(52)
        setPadding(dp(12), dp(10), dp(12), dp(10))
        setOnClickListener { action() }
    }
    private fun spaced(top: Int) = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(top) }
    private fun rounded(color: Int, radius: Int) = GradientDrawable().apply { setColor(color); cornerRadius = dp(radius).toFloat() }
    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
