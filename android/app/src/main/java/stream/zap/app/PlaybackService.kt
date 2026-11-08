package stream.zap.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.IBinder
import android.util.Log
import com.luminavideo.bridge.LuminaVideo

class PlaybackService : Service() {

    companion object {
        private const val TAG = "PlaybackService"
        const val CHANNEL_ID = "playback"
        private const val NOTIFICATION_ID = 1311
        private const val ACTION_START = "stream.zap.app.PLAYBACK_START"
        private const val ACTION_PAUSE = "stream.zap.app.PLAYBACK_PAUSE"
        private const val ACTION_PLAY = "stream.zap.app.PLAYBACK_PLAY"
        private const val ACTION_STOP = "stream.zap.app.PLAYBACK_STOP"
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_ARTIST = "artist"

        fun start(context: Context, title: String, artist: String) {
            val i = Intent(context, PlaybackService::class.java)
                .setAction(ACTION_START)
                .putExtra(EXTRA_TITLE, title)
                .putExtra(EXTRA_ARTIST, artist)
            try {
                if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i) else context.startService(i)
            } catch (e: Exception) {
                Log.w(TAG, "start failed: $e")
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, PlaybackService::class.java))
        }
    }

    private var session: MediaSession? = null
    private var title = ""
    private var artist = ""
    private var playing = true

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        if (Build.VERSION.SDK_INT >= 26) {
            val nm = getSystemService(NotificationManager::class.java)
            if (nm.getNotificationChannel(CHANNEL_ID) == null) {
                nm.createNotificationChannel(
                    NotificationChannel(CHANNEL_ID, "Playback", NotificationManager.IMPORTANCE_LOW)
                )
            }
        }
        session = MediaSession(this, "zap.stream").apply {
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() = setPlaying(true)
                override fun onPause() = setPlaying(false)
                override fun onStop() = stopSelf()
            })
            setSessionActivity(openAppIntent())
            isActive = true
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> {
                title = intent.getStringExtra(EXTRA_TITLE) ?: ""
                artist = intent.getStringExtra(EXTRA_ARTIST) ?: ""
                playing = true
                session?.setMetadata(
                    MediaMetadata.Builder()
                        .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                        .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
                        .putLong(MediaMetadata.METADATA_KEY_DURATION, -1L)
                        .build()
                )
            }
            ACTION_PAUSE -> setPlaying(false)
            ACTION_PLAY -> setPlaying(true)
            ACTION_STOP -> {
                LuminaVideo.pauseAll()
                stopSelf()
                return START_NOT_STICKY
            }
        }
        publish()
        return START_NOT_STICKY
    }

    private fun setPlaying(on: Boolean) {
        playing = on
        if (on) LuminaVideo.resumeAll() else LuminaVideo.pauseAll()
        publish()
    }

    private fun publish() {
        val state = if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED
        session?.setPlaybackState(
            PlaybackState.Builder()
                .setActions(
                    PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
                        PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP
                )
                .setState(state, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1f)
                .build()
        )
        val toggle = if (playing) {
            Notification.Action.Builder(android.R.drawable.ic_media_pause, "Pause", serviceIntent(ACTION_PAUSE)).build()
        } else {
            Notification.Action.Builder(android.R.drawable.ic_media_play, "Play", serviceIntent(ACTION_PLAY)).build()
        }
        val builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(this, CHANNEL_ID) else Notification.Builder(this)
        val n = builder
            .setSmallIcon(R.drawable.ic_stat_notify)
            .setContentTitle(title)
            .setContentText(artist)
            .setContentIntent(openAppIntent())
            .setDeleteIntent(serviceIntent(ACTION_STOP))
            .setOngoing(playing)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(toggle)
            .addAction(Notification.Action.Builder(android.R.drawable.ic_menu_close_clear_cancel, "Stop", serviceIntent(ACTION_STOP)).build())
            .setStyle(Notification.MediaStyle().setMediaSession(session?.sessionToken).setShowActionsInCompactView(0, 1))
            .build()
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIFICATION_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(NOTIFICATION_ID, n)
        }
    }

    private fun serviceIntent(action: String): PendingIntent =
        PendingIntent.getService(
            this, action.hashCode(), Intent(this, PlaybackService::class.java).setAction(action),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

    private fun openAppIntent(): PendingIntent =
        PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

    override fun onDestroy() {
        session?.isActive = false
        session?.release()
        session = null
        super.onDestroy()
    }
}
