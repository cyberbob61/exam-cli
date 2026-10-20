package com.cyberbob61.exam

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ModelTest {
    private fun correct(q: Question) = q.choices.indices.filter { q.choices[it].correct }

    @Test
    fun vceFormat() {
        val e = parseTxt(
            "Demo\n\nDesc\n\n1. Pick one.\n\nA. x\nB. y\n\nAnswer: B\n\nBecause.\n\n" +
                "2. Pick two. Steps:\n1. first\n2. second\n\nA. a\nB. b\nC. c\n\nAnswer: A and C\n"
        )
        assertEquals("Demo", e.title)
        assertEquals(2, e.questions.size)
        assertEquals(listOf(1), correct(e.questions[0]))
        assertEquals("Because.", e.questions[0].explanation)
        assertTrue(e.questions[1].text.contains("2. second"))
        assertEquals(listOf(0, 2), correct(e.questions[1]))
        assertTrue(e.questions[1].multiple)
        assertTrue(e.questions[1].isRight(setOf(0, 2)))
        assertFalse(e.questions[1].isRight(setOf(0)))
    }

    @Test
    fun qLineFormat() {
        val e = parseTxt(
            "Anthropic CCDV-F\nTotal questions: 2\nDownloaded: 2026-09-15 23:05\n=====\n\n" +
                "Q1\nFirst line.\n\nSecond paragraph?\n\nA. one\nB. two\n\nAnswer: B\n\n" +
                "Q2\nPick two.\n\nA. a\nB. b\nC. c\n\nAnswer: A, C\n"
        )
        assertEquals("Anthropic CCDV-F", e.title)
        assertFalse(e.description.contains('='))
        assertEquals("First line.\n\nSecond paragraph?", e.questions[0].text)
        assertEquals(listOf(1), correct(e.questions[0]))
        assertFalse(e.questions[0].multiple)
        assertEquals(listOf(0, 2), correct(e.questions[1]))
    }

    @Test
    fun cp1252Fallback() {
        val bytes = byteArrayOf(0x63, 0x61, 0x66, 0xE9.toByte(), 0x20, 0x93.toByte(), 0x68, 0x69, 0x94.toByte())
        assertEquals("café “hi”", decodeText(bytes))
        assertEquals("naïve", decodeText(byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()) + "naïve".toByteArray()))
    }

    @Test
    fun demoAndOldFiles() {
        // the demo the app ships comes from the shared samples folder
        val demo = readExam("demo.json", File("../../samples/demo.json").readBytes())
        assertEquals(3, demo.questions.size)
        assertEquals("DEMO-01", demo.id)
        val old = readExam("old.json", """{"title":"T","exams":[{"name":"Exam A","questions":[{"type":"fill","text":"x","choices":[{"text":"a","correct":true}]}]},{"questions":[{"text":"y","choices":[{"text":"b","correct":true}]}]}]}""".toByteArray())
        assertEquals(listOf("x", "y"), old.questions.map { it.text })
        assertFalse(old.questions[0].multiple)
        assertEquals("T", old.id)
    }
}
