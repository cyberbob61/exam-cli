package com.cyberbob61.exam

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class LibraryTest {
    @Test
    fun deleteRemovesJsonAndItsTxtOnly() {
        val dir = Files.createTempDirectory("exams").toFile()
        listOf("a.json", "a.txt", "b.json", "ab.json").forEach { File(dir, it).writeText("{}") }

        deleteExam(dir, File(dir, "a.json"))

        // the .txt goes too, or the next scan would make a.json again; other exams stay
        assertEquals(listOf("ab.json", "b.json"), dir.list()!!.sorted())
        dir.deleteRecursively()
    }

    @Test
    fun deletedDemoIsRemembered() {
        val dir = Files.createTempDirectory("exams").toFile()
        File(dir, "demo.json").writeText("{}")

        deleteExam(dir, File(dir, "demo.json"))

        assertTrue(File(dir, DEMO_GONE).exists())
        assertEquals(listOf(DEMO_GONE), dir.list()!!.toList())
        dir.deleteRecursively()
    }
}
