package cl.matiaspalma.everythingimu.core.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.PowerManager
import android.net.wifi.WifiManager
import androidx.core.app.NotificationCompat
import androidx.core.content.getSystemService
import androidx.lifecycle.LifecycleService
import cl.matiaspalma.everythingimu.core.tracking.TrackingController

class TrackingService : LifecycleService() {

    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannel()
        TrackingController.ensureInit(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> {
                stopTracking()
                return START_NOT_STICKY
            }
            ACTION_RECENTER -> {
                TrackingController.sendRecenter()
                return START_STICKY
            }
        }
        startTracking()
        return START_STICKY
    }

    private fun startTracking() {
        startForegroundCompat()
        acquireWakeLock()
        acquireWifiLock()
        TrackingController.onServiceStart()
    }

    private fun stopTracking() {
        TrackingController.onServiceStop()
        releaseWakeLock()
        releaseWifiLock()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(true)
        }
        stopSelf()
    }

    override fun onDestroy() {
        TrackingController.onServiceStop()
        releaseWakeLock()
        releaseWifiLock()
        super.onDestroy()
    }

    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return
        val pm = getSystemService<PowerManager>() ?: return
        wakeLock = pm.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "everythingimu:tracking",
        ).apply {
            setReferenceCounted(false)
            // No timeout — released explicitly on stopTracking. Tracking
            // legitimately runs for the full VR session.
            acquire()
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
    }

    private fun acquireWifiLock() {
        if (wifiLock?.isHeld == true) return
        val wifi = getSystemService<WifiManager>() ?: return
        // LOW_LATENCY (API 29+) keeps Wi-Fi awake with the screen off and cuts
        // UDP latency — critical on Wear OS, which otherwise parks Wi-Fi and
        // routes through the Bluetooth companion proxy. HIGH_PERF is the
        // pre-29 fallback.
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        try {
            wifiLock = wifi.createWifiLock(mode, "everythingimu:wifi").apply {
                setReferenceCounted(false)
                acquire()
            }
        } catch (_: Throwable) {
            wifiLock = null
        }
    }

    private fun releaseWifiLock() {
        wifiLock?.let { if (it.isHeld) it.release() }
        wifiLock = null
    }

    private fun startForegroundCompat() {
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildNotification(): Notification {
        val stopIntent = Intent(this, TrackingService::class.java).apply { action = ACTION_STOP }
        val stopPi = PendingIntent.getService(
            this, 0, stopIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val recenterIntent = Intent(this, TrackingService::class.java).apply { action = ACTION_RECENTER }
        val recenterPi = PendingIntent.getService(
            this, 1, recenterIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("everything-imu tracking")
            .setContentText("IMU streaming · long-press vol or shake to recenter")
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .addAction(0, "Recenter", recenterPi)
            .addAction(0, "Stop", stopPi)
            .build()
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val mgr = getSystemService<NotificationManager>() ?: return
        if (mgr.getNotificationChannel(CHANNEL_ID) != null) return
        mgr.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "Tracking",
                NotificationManager.IMPORTANCE_LOW,
            ).apply { description = "Foreground tracking notification" },
        )
    }

    companion object {
        const val ACTION_STOP = "cl.matiaspalma.everythingimu.action.STOP"
        const val ACTION_RECENTER = "cl.matiaspalma.everythingimu.action.RECENTER"
        private const val CHANNEL_ID = "tracking"
        private const val NOTIFICATION_ID = 1001

        fun start(context: Context) {
            val intent = Intent(context, TrackingService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, TrackingService::class.java).apply { action = ACTION_STOP }
            context.startService(intent)
        }
    }
}
