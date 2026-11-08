package stream.zap.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import androidx.work.Constraints
import androidx.work.NetworkType
import org.json.JSONArray
import java.io.File
import java.util.concurrent.TimeUnit

class LiveCheckWorker(context: Context, params: WorkerParameters) : Worker(context, params) {

    companion object {
        private const val TAG = "LiveCheckWorker"
        const val CHANNEL_ID = "go_live"
        private const val WORK_NAME = "live-check"

        init {
            try {
                System.loadLibrary("zap_stream_app")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "native library missing: ${e.message}")
            }
        }

        fun ensureChannel(context: Context) {
            if (Build.VERSION.SDK_INT < 26) return
            val nm = context.getSystemService(NotificationManager::class.java)
            if (nm.getNotificationChannel(CHANNEL_ID) == null) {
                val ch = NotificationChannel(CHANNEL_ID, "Go live alerts", NotificationManager.IMPORTANCE_DEFAULT)
                ch.description = "A streamer you asked to hear about went live"
                nm.createNotificationChannel(ch)
            }
        }

        fun schedule(context: Context, enabled: Boolean) {
            val wm = WorkManager.getInstance(context)
            if (!enabled) {
                wm.cancelUniqueWork(WORK_NAME)
                return
            }
            val constraints = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
            val req = PeriodicWorkRequestBuilder<LiveCheckWorker>(15, TimeUnit.MINUTES)
                .setConstraints(constraints)
                .build()
            wm.enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, req)
        }

        fun runNow(context: Context) {
            WorkManager.getInstance(context).enqueueUniqueWork(
                "$WORK_NAME-now",
                androidx.work.ExistingWorkPolicy.KEEP,
                OneTimeWorkRequestBuilder<LiveCheckWorker>().build()
            )
        }

        fun settingsDir(context: Context): File = File(context.filesDir, "settings")

        fun hasWatched(context: Context): Boolean {
            val f = File(settingsDir(context), "notify.txt")
            return f.exists() && f.readLines().any { it.startsWith("watch ") }
        }
    }

    private external fun nativeCheck(settingsDir: String): String

    override fun doWork(): Result {
        val out = try {
            nativeCheck(settingsDir(applicationContext).absolutePath)
        } catch (e: Throwable) {
            Log.w(TAG, "check failed: $e")
            return Result.retry()
        }
        val alerts = try { JSONArray(out) } catch (e: Exception) { JSONArray() }
        Log.i(TAG, "live check: ${alerts.length()} alert(s)")
        if (alerts.length() == 0) return Result.success()
        ensureChannel(applicationContext)
        if (Build.VERSION.SDK_INT >= 33 &&
            applicationContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            Log.w(TAG, "notification permission not granted")
            return Result.success()
        }
        val nm = NotificationManagerCompat.from(applicationContext)
        for (i in 0 until alerts.length()) {
            val a = alerts.getJSONObject(i)
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(a.getString("url")))
                .setClass(applicationContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            val id = a.getString("id").hashCode()
            val pending = PendingIntent.getActivity(
                applicationContext, id, intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            val n = NotificationCompat.Builder(applicationContext, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_stat_notify)
                .setContentTitle(a.getString("title"))
                .setContentText(a.optString("body"))
                .setCategory(NotificationCompat.CATEGORY_SOCIAL)
                .setAutoCancel(true)
                .setContentIntent(pending)
                .build()
            try {
                nm.notify(id, n)
            } catch (e: SecurityException) {
                Log.w(TAG, "notify refused: $e")
            }
        }
        return Result.success()
    }
}
