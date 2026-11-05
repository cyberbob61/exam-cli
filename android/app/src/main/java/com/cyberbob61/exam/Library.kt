package com.cyberbob61.exam

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File

/** marker left in the exam folder once the demo is deleted, so it is not copied back */
internal const val DEMO_GONE = ".demo-deleted"

/** removes an exam's .json and the .txt it was made from (or the next scan would rebuild the .json) */
internal fun deleteExam(dir: File, file: File) {
    val stem = file.nameWithoutExtension
    dir.listFiles().orEmpty()
        .filter { it.nameWithoutExtension == stem && it.extension.lowercase() in setOf("json", "txt") }
        .forEach { it.delete() }
    if (stem == "demo") File(dir, DEMO_GONE).createNewFile()
}

/** a list row: an exam file, or a .txt that could not be converted */
class Entry(val file: File, val exam: Exam?, val error: String?)

/** the app's own exam folder: the demo, plus every file brought in with Open file */
class Library(private val context: Context) {
    private val dir = File(context.filesDir, "exams").apply { mkdirs() }

    /** converts each .txt without a .json next to it (the .txt stays, a .json is never overwritten), then lists the .json exams */
    fun scan(): List<Entry> {
        val demo = File(dir, "demo.json")
        if (!demo.exists() && !File(dir, DEMO_GONE).exists()) context.assets.open("demo.json").use { input -> demo.outputStream().use { input.copyTo(it) } }
        val files = { ext: String -> dir.listFiles().orEmpty().filter { it.extension.equals(ext, ignoreCase = true) }.sortedBy { it.name.lowercase() } }
        val failed = files("txt").mapNotNull { txt ->
            runCatching {
                val j = File(dir, txt.nameWithoutExtension + ".json")
                if (!j.exists()) j.writeText(json.encodeToString(Exam.serializer(), readExam(txt.name, txt.readBytes())))
            }.exceptionOrNull()?.let { Entry(txt, null, "cannot convert: ${it.message}") }
        }
        val exams = files("json").map { j ->
            runCatching { readExam(j.name, j.readBytes()) }.fold({ Entry(j, it, null) }, { Entry(j, null, it.message ?: "cannot read") })
        }
        return exams + failed
    }

    fun delete(e: Entry) = deleteExam(dir, e.file)

    /** copies a picked .json or .txt into the library; returns an error message, or null when it worked */
    fun import(uri: Uri): String? = runCatching {
        val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "exam.json"
        require(name.endsWith(".json", true) || name.endsWith(".txt", true)) { "Only .json and .txt exams can be opened." }
        val bytes = context.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
        readExam(name, bytes) // refuse files that are not exams before keeping them
        File(dir, File(name).name).writeBytes(bytes)
    }.exceptionOrNull()?.let { "Cannot open this file: ${it.message}" }
}
