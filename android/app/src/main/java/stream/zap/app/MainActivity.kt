/**
 * lumina-video Demo Activity
 *
 * Main activity for the lumina-video demo application. This activity uses GameActivity
 * from the Android Game SDK to host the native Rust/egui application.
 *
 * # Architecture
 *
 * ```
 * MainActivity (Kotlin)
 *     |
 *     +--> LuminaVideo.init(this)  [one-time, stores applicationContext]
 *     |
 *     +--> GameActivity (androidx.games)
 *             |
 *             +--> libzap_stream_app.so (Rust + egui + wgpu)
 *                     |
 *                     +--> LuminaVideo.createPlayer(nativeHandle) [via JNI]
 *                             |
 *                             +--> ExoPlayerBridge + ExoPlayer [on HandlerThread]
 *                                     |
 *                                     +--> HardwareBuffer (zero-copy frames)
 * ```
 *
 * # Video Playback Flow
 *
 * 1. `LuminaVideo.init(this)` stores the application context
 * 2. Rust's `AndroidVideoDecoder::new()` calls `LuminaVideo.createPlayer()` via JNI
 * 3. `createPlayer()` creates ExoPlayer on a dedicated HandlerThread
 * 4. ExoPlayerBridge receives decoded frames as HardwareBuffers
 * 5. Bridge submits HardwareBuffers to Rust via nativeSubmitHardwareBuffer
 * 6. Rust imports HardwareBuffer into Vulkan via VK_ANDROID_external_memory
 * 7. egui displays the texture with zero CPU copies
 */
package stream.zap.app

import android.content.Intent
import android.app.PictureInPictureParams
import android.content.pm.ActivityInfo
import android.os.Bundle
import android.util.Rational
import java.io.File
import android.util.Log
import android.view.WindowManager
import android.view.View
import android.view.ViewGroup
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.google.androidgamesdk.GameActivity
import com.luminavideo.bridge.LuminaVideo

/**
 * Main activity hosting the lumina-video native application.
 *
 * Extends GameActivity which provides the native window surface
 * and input handling required by egui's android-game-activity feature.
 */
class MainActivity : GameActivity() {

    companion object {
        private const val TAG = "LuminaVideoDemo"

        init {
            try {
                System.loadLibrary("zap_stream_app")
                Log.i(TAG, "Loaded libzap_stream_app.so")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load libzap_stream_app.so: ${e.message}")
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        LuminaVideo.init(this)
        setupInsets()
        super.onCreate(savedInstanceState)
        handleLink(intent)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    fun setLandscapeFullscreen(enabled: Boolean) {
        runOnUiThread {
            requestedOrientation = if (enabled) {
                ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            } else {
                ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
            val controller = WindowInsetsControllerCompat(window, window.decorView)
            if (enabled) {
                controller.systemBarsBehavior =
                    WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                controller.hide(WindowInsetsCompat.Type.systemBars())
            } else {
                controller.show(WindowInsetsCompat.Type.systemBars())
            }
        }
    }

    fun openUrl(url: String): Boolean {
        return try {
            val intent = Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url))
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            startActivity(intent)
            true
        } catch (e: Exception) {
            Log.w("ZapStream", "openUrl failed: " + e)
            false
        }
    }

    fun getClipboardText(): String {
        val cm = getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
        val clip = cm.primaryClip ?: return ""
        if (clip.itemCount == 0) return ""
        return clip.getItemAt(0).coerceToText(this)?.toString() ?: ""
    }

    fun setClipboardText(text: String) {
        runOnUiThread {
            val cm = getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
            cm.setPrimaryClip(android.content.ClipData.newPlainText("zap.stream", text))
        }
    }

    fun amberRequest(json: String): Boolean = Nip55Signer.request(this, json)

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (!Nip55Signer.onActivityResult(requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data)
        }
    }

    fun enterPip() {
        runOnUiThread {
            val params = PictureInPictureParams.Builder()
                .setAspectRatio(Rational(16, 9))
                .build()
            enterPictureInPictureMode(params)
        }
    }

    private fun handleLink(intent: Intent?) {
        val data = intent?.data?.toString() ?: return
        val dir = File(filesDir, "settings")
        dir.mkdirs()
        File(dir, "pending_link").writeText(data)
    }

    private var insetLeft = 0

    private external fun nativeSetInsets(top: Int, rects: IntArray)

    override fun dispatchTouchEvent(ev: android.view.MotionEvent): Boolean {
        ev.offsetLocation(-insetLeft.toFloat(), 0f)
        return super.dispatchTouchEvent(ev)
    }

    private fun setupInsets() {
        val content = window.decorView.findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { v, windowInsets ->
            val insets = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime()
            )
            val lp = v.layoutParams as ViewGroup.MarginLayoutParams
            lp.topMargin = 0
            lp.leftMargin = insets.left
            lp.bottomMargin = insets.bottom
            lp.rightMargin = insets.right
            insetLeft = insets.left
            v.layoutParams = lp
            val rects = ArrayList<Int>()
            windowInsets.displayCutout?.boundingRects?.forEach { r ->
                if (r.top <= 0 && r.bottom <= v.rootView.height / 4) {
                    rects.add(r.left - insets.left)
                    rects.add(r.right - insets.left)
                    rects.add(r.bottom)
                }
            }
            try {
                nativeSetInsets(insets.top, rects.toIntArray())
            } catch (e: UnsatisfiedLinkError) {
                Log.w(TAG, "nativeSetInsets missing")
            }
            windowInsets
        }
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = android.graphics.Color.TRANSPARENT
        if (android.os.Build.VERSION.SDK_INT >= 28) {
            window.attributes.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        WindowInsetsControllerCompat(window, window.decorView).isAppearanceLightStatusBars = false
    }
}
