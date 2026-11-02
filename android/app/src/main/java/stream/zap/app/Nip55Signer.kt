package stream.zap.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.util.Log
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

object Nip55Signer {
    private const val TAG = "Nip55Signer"
    private const val REQUEST_BASE = 55000

    private val nextCode = AtomicInteger(REQUEST_BASE)
    private val inflight = ConcurrentHashMap<Int, String>()

    @JvmStatic
    external fun nativeReply(id: String, json: String)

    fun reply(id: String, fields: Map<String, Any?>) {
        val out = JSONObject()
        for ((k, v) in fields) {
            if (v != null) out.put(k, v)
        }
        nativeReply(id, out.toString())
    }

    fun parseResult(resultCode: Int, data: Intent?, requestId: String): Map<String, Any?> {
        if (resultCode != Activity.RESULT_OK) {
            return mapOf("id" to requestId, "code" to resultCode)
        }
        val rejected = data?.getBooleanExtra("rejected", false) == true ||
            data?.getStringExtra("rejected") == "true"
        return mapOf(
            "id" to requestId,
            "code" to -1,
            "result" to (data?.getStringExtra("result") ?: data?.getStringExtra("signature")),
            "event" to data?.getStringExtra("event"),
            "package" to data?.getStringExtra("package"),
            "rejected" to rejected,
        )
    }

    fun isInstalled(activity: Activity): Boolean {
        val probe = Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:"))
        return activity.packageManager.queryIntentActivities(probe, 0).isNotEmpty()
    }

    fun request(activity: Activity, json: String): Boolean {
        val req = JSONObject(json)
        val id = req.getString("id")
        val provider = req.optJSONObject("provider")
        if (provider != null) {
            Thread {
                val fromProvider = queryProvider(activity, provider)
                if (fromProvider != null) {
                    reply(id, fromProvider)
                } else {
                    activity.runOnUiThread { launchIntent(activity, id, req) }
                }
            }.start()
        } else {
            activity.runOnUiThread { launchIntent(activity, id, req) }
        }
        return true
    }

    private fun queryProvider(activity: Activity, provider: JSONObject): Map<String, Any?>? {
        return try {
            val args = provider.getJSONArray("args").let { a -> Array(a.length()) { a.getString(it) } }
            val uri = Uri.parse("content://" + provider.getString("authority"))
            val cursor = activity.contentResolver.query(uri, null, null, args, null) ?: return null
            cursor.use { c ->
                if (c.getColumnIndex("rejected") > -1) {
                    return mapOf("code" to -1, "rejected" to true)
                }
                if (!c.moveToFirst()) return null
                val result = c.getColumnIndex("result").takeIf { it >= 0 }?.let { c.getString(it) }
                val event = c.getColumnIndex("event").takeIf { it >= 0 }?.let { c.getString(it) }
                if (result == null && event == null) return null
                mapOf("code" to -1, "result" to result, "event" to event)
            }
        } catch (e: Exception) {
            Log.w(TAG, "provider query failed: $e")
            null
        }
    }

    private fun launchIntent(activity: Activity, id: String, req: JSONObject) {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(req.getString("uri")))
        if (!req.isNull("package")) intent.`package` = req.getString("package")
        val extras = req.getJSONObject("extras")
        for (key in extras.keys()) intent.putExtra(key, extras.getString(key))
        intent.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val code = nextCode.incrementAndGet()
        inflight[code] = id
        try {
            @Suppress("DEPRECATION")
            activity.startActivityForResult(intent, code)
        } catch (e: Exception) {
            Log.w(TAG, "no signer: $e")
            inflight.remove(code)
            reply(id, mapOf("error" to "not_installed"))
        }
    }

    fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        val id = inflight.remove(requestCode) ?: return false
        reply(id, parseResult(resultCode, data, id))
        return true
    }
}
