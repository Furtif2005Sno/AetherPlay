package fr.jolannrt.aetherplay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.content.ContextCompat

/**
 * Foreground service ("AetherPlay manette" notification): Android keeps the
 * app running, so the Bluetooth controller and the Wi-Fi link stay connected
 * while the screen is locked or another app is open. Its "Arrêter" action
 * disconnects everything.
 */
class ControllerService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            Controller.shutdown()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        Controller.init(this)
        val notification = notification()
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        Controller.discovery.start()
        return START_STICKY
    }

    private fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Manette connectée", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Garde la manette connectée à AetherPlay en arrière-plan."
            },
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this, 1, Intent(this, ControllerService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setContentTitle("AetherPlay manette")
            .setContentText("Reste connectée à l'ordinateur")
            .setContentIntent(open)
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Arrêter", stop).build())
            .build()
    }

    companion object {
        private const val CHANNEL = "controller"
        private const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "fr.jolannrt.aetherplay.STOP"

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, ControllerService::class.java))
        }
    }
}
