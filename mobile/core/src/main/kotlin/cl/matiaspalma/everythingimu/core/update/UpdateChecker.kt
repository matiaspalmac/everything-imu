package cl.matiaspalma.everythingimu.core.update

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Queries the GitHub Releases API for the monorepo's latest tag and reports
 * whether it is newer than the running build. Network-only — no install is
 * performed; the caller hands the user an `Intent.ACTION_VIEW` deep-link to
 * the release's HTML page so the OS package installer takes it from there.
 *
 * Lives in `core` so the phone and Wear OS apps can share the network call
 * and the version-comparison logic; only the UI surface differs.
 */
object UpdateChecker {

    /** Resolved comparison result between the running build and GitHub's latest tag. */
    data class UpdateInfo(
        val currentVersion: String,
        val latestVersion: String,
        val releaseUrl: String,
        val updateAvailable: Boolean,
    )

    private const val LATEST_RELEASE_URL =
        "https://api.github.com/repos/matiaspalmac/everything-imu/releases/latest"

    /**
     * Fetches `releases/latest` and returns a populated [UpdateInfo]. Returns
     * `null` only on transport failure — when the request succeeds but no
     * newer version is published, the result carries `updateAvailable = false`
     * so the UI can show "you're on the latest" without a second flag.
     */
    suspend fun check(currentVersion: String): UpdateInfo? = withContext(Dispatchers.IO) {
        runCatching {
            val conn = (URL(LATEST_RELEASE_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "GET"
                connectTimeout = 5_000
                readTimeout = 5_000
                setRequestProperty("Accept", "application/vnd.github+json")
                setRequestProperty("User-Agent", "everything-imu-mobile")
            }
            try {
                if (conn.responseCode !in 200..299) return@runCatching null
                val body = conn.inputStream.bufferedReader().use { it.readText() }
                val root = JSONObject(body)
                val tag = root.optString("tag_name").trimStart('v').ifEmpty { return@runCatching null }
                val htmlUrl = root.optString("html_url")
                UpdateInfo(
                    currentVersion = currentVersion,
                    latestVersion = tag,
                    releaseUrl = htmlUrl,
                    updateAvailable = isNewer(tag, currentVersion),
                )
            } finally {
                conn.disconnect()
            }
        }.getOrNull()
    }

    /**
     * SemVer-ish comparison that strips non-digit separators and compares the
     * remaining integer components lexicographically. Falls back to "longer is
     * newer" when prefixes match (e.g. `1.0.1` > `1.0`). Matches the desktop
     * `semver_newer` helper so phone, watch, and Tauri agree on ordering.
     */
    private fun isNewer(candidate: String, current: String): Boolean {
        val a = parseComponents(candidate)
        val b = parseComponents(current)
        val min = minOf(a.size, b.size)
        for (i in 0 until min) {
            if (a[i] > b[i]) return true
            if (a[i] < b[i]) return false
        }
        return a.size > b.size
    }

    private fun parseComponents(s: String): List<Int> =
        s.split(Regex("[^0-9]+")).mapNotNull { it.toIntOrNull() }
}
