package com.cyberbob61.exam

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** CI replaces this pre-release with the newest master build on every push */
private const val LATEST = "https://api.github.com/repos/cyberbob61/exam-cli/releases/tags/android-latest"

class Update(val build: Int, val apk: String)

/** reads the release JSON: an "Android build N" release with an .apk, newer than [current], is an update */
internal fun parseUpdate(release: String, current: Int): Update? {
    val o = json.parseToJsonElement(release).jsonObject
    val build = Regex("""build (\d+)""").find(o["name"]?.jsonPrimitive?.content.orEmpty())?.groupValues?.get(1)?.toInt() ?: return null
    // the asset's API url, not browser_download_url: github.com/.../releases/download has answered 504 while the API worked
    val apk = o["assets"]?.jsonArray.orEmpty().map { it.jsonObject }
        .firstOrNull { it["name"]?.jsonPrimitive?.content.orEmpty().endsWith(".apk") }
        ?.get("url")?.jsonPrimitive?.content ?: return null
    return Update(build, apk).takeIf { build > current }
}

/** null when there is nothing newer, or no network: the check must never get in the way */
fun checkUpdate(): Update? = runCatching { parseUpdate(URL(LATEST).readText(), BuildConfig.VERSION_CODE) }.getOrNull()

/** the asset API answers with the file itself (a redirect to it) only when asked for octet-stream */
fun downloadApk(context: Context, url: String): File {
    val c = URL(url).openConnection() as HttpURLConnection
    c.setRequestProperty("Accept", "application/octet-stream")
    if (c.responseCode != 200) throw IOException("GitHub answered HTTP ${c.responseCode}, try again later")
    return File(context.cacheDir, "update.apk").apply { c.inputStream.use { i -> outputStream().use { i.copyTo(it) } } }
}

/** hands the APK to the system installer (it asks once to allow installing from this app) */
fun installApk(context: Context, apk: File) {
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", apk)
    context.startActivity(
        Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
    )
}
