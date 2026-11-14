package org.zacharie.iris.kiosk

import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.WindowManager
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : Activity() {

    private lateinit var webView: WebView
    private val prefs by lazy { getSharedPreferences("iris_kiosk", Context.MODE_PRIVATE) }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        applyImmersiveMode()

        val root = FrameLayout(this).apply {
            setBackgroundColor(Color.parseColor("#090D16"))
        }

        webView = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#090D16"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.useWideViewPort = true
            settings.loadWithOverviewMode = true
            settings.mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
            webViewClient = object : WebViewClient() {
                override fun onReceivedError(
                    view: WebView?,
                    request: WebResourceRequest?,
                    error: WebResourceError?
                ) {
                    if (request?.isForMainFrame == true) {
                        view?.postDelayed({ view.reload() }, 4000)
                    }
                }
            }
        }

        val serverBtn = Button(this).apply {
            text = "⚙"
            textSize = 14f
            alpha = 0.45f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#1E293B"))
            setOnClickListener { promptServerUrl() }
        }

        val btnParams = FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.WRAP_CONTENT,
            FrameLayout.LayoutParams.WRAP_CONTENT
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.END
            setMargins(16, 16, 16, 16)
        }

        root.addView(
            webView,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT
            )
        )
        root.addView(serverBtn, btnParams)
        setContentView(root)

        webView.loadUrl(getServerUrl())
    }

    private fun getServerUrl(): String =
        prefs.getString("server_url", DEFAULT_SERVER_URL) ?: DEFAULT_SERVER_URL

    private fun promptServerUrl() {
        val input = EditText(this).apply {
            setText(getServerUrl())
            setSingleLine(true)
        }
        AlertDialog.Builder(this)
            .setTitle("URL du serveur Rust Iris")
            .setMessage("Ex: http://127.0.0.1:8080 (adb reverse) ou https://iris.zacharie.org")
            .setView(input)
            .setPositiveButton("Enregistrer") { _, _ ->
                val url = input.text.toString().trim().ifEmpty { DEFAULT_SERVER_URL }
                prefs.edit().putString("server_url", url).apply()
                webView.loadUrl(url)
            }
            .setNegativeButton("Annuler", null)
            .show()
    }

    private fun applyImmersiveMode() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val controller = WindowInsetsControllerCompat(window, window.decorView)
        controller.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.systemBars())
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) applyImmersiveMode()
    }

    companion object {
        private const val DEFAULT_SERVER_URL = "http://127.0.0.1:8080"
    }
}
