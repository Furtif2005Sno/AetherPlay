package fr.jolannrt.aetherplay

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File

/**
 * Updates of the app itself from the GitHub releases (the same ones as
 * AetherPlay): a newer AetherPlay-Android.apk is downloaded and handed to
 * Android's installer, which asks the user to confirm.
 */
object AppUpdate {
    private const val LATEST = "https://api.github.com/repos/Furtif2005Sno/AetherPlay/releases/latest"
    private val http = OkHttpClient()

    data class Release(val version: String, val apkUrl: String)

    /** The latest release if it is newer than this app, else null. */
    suspend fun check(context: Context): Release? = withContext(Dispatchers.IO) {
        runCatching {
            val body = http.newCall(Request.Builder().url(LATEST).header("Accept", "application/vnd.github+json").build())
                .execute().use { it.body?.string() } ?: return@runCatching null
            val json = JSONObject(body)
            val version = json.getString("tag_name").removePrefix("v")
            val assets = json.getJSONArray("assets")
            val apk = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.getString("name") == "AetherPlay-Android.apk" }
                ?.getString("browser_download_url") ?: return@runCatching null
            val current = context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "0"
            if (newer(version, current)) Release(version, apk) else null
        }.getOrNull()
    }

    /** Downloads the APK and opens Android's installer on it. */
    suspend fun install(context: Context, release: Release) {
        val file = withContext(Dispatchers.IO) {
            val dir = File(context.cacheDir, "updates").apply { mkdirs() }
            val out = File(dir, "AetherPlay-${release.version}.apk")
            http.newCall(Request.Builder().url(release.apkUrl).build()).execute().use { res ->
                check(res.isSuccessful) { "HTTP ${res.code}" }
                res.body!!.byteStream().use { input -> out.outputStream().use { input.copyTo(it) } }
            }
            out
        }
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    /** "0.3.2" > "0.3.1" (numeric parts compared in order). */
    fun newer(candidate: String, current: String): Boolean {
        val a = candidate.split('.', '-').map { it.toIntOrNull() ?: 0 }
        val b = current.split('.', '-').map { it.toIntOrNull() ?: 0 }
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }
}
