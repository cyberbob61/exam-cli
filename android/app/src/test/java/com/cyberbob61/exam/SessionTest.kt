package com.cyberbob61.exam

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionTest {
    private fun q(vararg correct: Boolean, multiple: Boolean = false) =
        Question(type = if (multiple) "multiple" else "single", text = "?", choices = correct.map { Choice("x", it) })

    @Test
    fun answersCountWithoutPressingCheck() {
        val s = Session(Exam(questions = listOf(q(false, true), q(true, false, true, multiple = true), q(true, false))), alwaysExplain = false)
        // pick the right answers and just swipe on, never pressing the check button
        s.toggle(0, 1)
        s.toggle(1, 0)
        s.toggle(1, 2)
        assertTrue(s.isRight(0))
        assertTrue(s.isRight(1))
        assertFalse(s.isRight(2)) // nothing chosen: unanswered, counted as incorrect like VCE
        assertEquals(1, (0 until s.n).count { !s.isRight(it) })
        s.toggle(2, 1)
        assertFalse(s.isRight(2))
    }

    @Test
    fun alwaysShowAnswersStartsChecked() {
        val exam = Exam(questions = listOf(q(false, true), q(true, false)))
        assertTrue(Session(exam, alwaysExplain = true, alwaysShow = true).shown.all { it })
        assertTrue(Session(exam, alwaysExplain = true).shown.none { it })
        // shown answers are not given answers: nothing counts as right until it is picked
        assertFalse(Session(exam, alwaysExplain = false, alwaysShow = true).isRight(0))
    }
}
