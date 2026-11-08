// Exam data, same JSON as exam-cli and the Exam designer (src/model.rs), plus the TXT formats they import.
package com.cyberbob61.exam

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.Charset
import java.nio.charset.CodingErrorAction

@Serializable
data class Choice(val text: String = "", val correct: Boolean = false)

@Serializable
data class Question(
    /** "single" or "multiple"; "fill" from older files opens as single */
    val type: String = "single",
    val text: String = "",
    /** data: URL, so one .json file carries everything */
    val image: String = "",
    val choices: List<Choice> = emptyList(),
    val explanation: String = "",
    val shuffle: Boolean = true,
) {
    val multiple get() = type == "multiple"

    fun isRight(picks: Set<Int>) = choices.indices.all { choices[it].correct == (it in picks) }
}

/** version 0.2 files held several exams ("Exam A", "Exam B"); read only to merge them */
@Serializable
data class SubExam(val name: String = "", val questions: List<Question> = emptyList())

@Serializable
data class Exam(
    val title: String = "Untitled Exam",
    val code: String = "000-000",
    val version: String = "1.0",
    val description: String = "",
    val showDescription: Boolean = false,
    /** out of 1000, as in VCE */
    val passingScore: Int = 800,
    /** minutes, 0 = no limit */
    val timeLimit: Int = 120,
    val questions: List<Question> = emptyList(),
    val exams: List<SubExam> = emptyList(),
) {
    /** merges the exams of older files into one question list */
    fun fixup() = copy(questions = questions + exams.flatMap { it.questions }, exams = emptyList())

    /** "CCIE-ENT", or the title when the exam has no real number */
    val id get() = if (code.isNotBlank() && code != "000-000") code else title
}

val json = Json {
    ignoreUnknownKeys = true
    prettyPrint = true
}

fun letter(i: Int) = 'A' + i

// ---------- TXT: VCE ("1. text / A. option / Answer: A, C") and "Q1 / text / A. option / Answer: B" ----------

data class TxtExam(val title: String, val description: String, val questions: List<Question>)

fun TxtExam.toExam() = Exam(title = title, description = description, questions = questions)

private fun joinLines(v: List<String>): String {
    var s = v.joinToString("\n")
    while ("\n\n\n" in s) s = s.replace("\n\n\n", "\n\n")
    return s.trim()
}

private val qAlone = Regex("""Q(\d+)\s*""")
private val qNumbered = Regex("""(\d+)\.\s+(.*)""")
private val choiceLine = Regex("""([A-Z])\.\s+(.*)""")

/** "Q12" alone (text on the next lines) or "12. text" -> (12, text) */
private fun qStart(line: String): Pair<Int, String>? {
    qAlone.matchEntire(line)?.let { m -> return m.groupValues[1].toIntOrNull()?.let { it to "" } }
    qNumbered.matchEntire(line)?.let { m -> return m.groupValues[1].toIntOrNull()?.let { it to m.groupValues[2] } }
    return null
}

private enum class St { Text, Choices, Expl }

private class Draft(val text: MutableList<String>) {
    val choices = mutableListOf<Choice>()
    var answer = ""
    val expl = mutableListOf<String>()

    fun finish(): Question {
        // single capital letters in "A, B and F"
        val right = answer.split(Regex("""[^\p{L}\p{N}]""")).filter { it.length == 1 && it[0] in 'A'..'Z' }.map { it[0] - 'A' }.toSet()
        val marked = choices.mapIndexed { j, c -> c.copy(correct = j in right) }
        return Question(
            type = if (marked.count { it.correct } > 1) "multiple" else "single",
            text = joinLines(text),
            choices = marked,
            explanation = joinLines(expl),
        )
    }
}

fun parseTxt(source: String): TxtExam {
    val src = source.removePrefix("﻿").replace("\r", "")
    val questions = mutableListOf<Question>()
    val head = mutableListOf<String>()
    var d: Draft? = null
    var st = St.Text
    for (line in src.split('\n')) {
        val start = qStart(line)
        // only the next expected number starts a question, so numbered lists inside text stay text;
        // after the choices too, for a question that has no "Answer:" line
        if (start != null && (d == null || st != St.Text) &&start.first == questions.size + (if (d != null) 2 else 1)) {
            d?.let { questions += it.finish() }
            d = Draft(mutableListOf(start.second))
            st = St.Text
            continue
        }
        val cur = d
        if (cur == null) {
            // "=====" rules under the header are decoration
            if (!(line.length >= 3 && line.all { it == '=' })) head += line
            continue
        }
        if (st != St.Expl) {
            if (line.length >= 7 && line.substring(0, 7).equals("answer:", ignoreCase = true)) {
                cur.answer = line.substring(7).trim()
                st = St.Expl
                continue
            }
            val c = choiceLine.matchEntire(line)
            if (c != null && cur.choices.size < 26 && c.groupValues[1][0] == letter(cur.choices.size)) {
                cur.choices += Choice(c.groupValues[2])
                st = St.Choices
                continue
            }
        }
        when (st) {
            St.Choices -> if (line.isNotBlank()) {
                val k = cur.choices.lastIndex
                cur.choices[k] = cur.choices[k].copy(text = cur.choices[k].text + "\n" + line)
            }
            St.Expl -> cur.expl += line
            St.Text -> cur.text += line
        }
    }
    d?.let { questions += it.finish() }
    // a question without two options or without an answer (e.g. HOTSPOT) cannot be played
    questions.retainAll { q -> q.choices.size >= 2 && q.choices.any { it.correct } }
    val h = joinLines(head).split('\n')
    return TxtExam(h.first().trim(), joinLines(h.drop(1)), questions)
}

/** UTF-8 if valid, otherwise Windows-1252 (what older Windows editors save English text as) */
fun decodeText(bytes: ByteArray): String {
    val b = if (bytes.size >= 3 && bytes[0] == 0xEF.toByte() && bytes[1] == 0xBB.toByte() && bytes[2] == 0xBF.toByte()) bytes.copyOfRange(3, bytes.size) else bytes
    return try {
        Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(b)).toString()
    } catch (e: CharacterCodingException) {
        String(b, Charset.forName("windows-1252"))
    }
}

/** reads an exam: our .json, or a .txt in one of the formats above */
fun readExam(name: String, bytes: ByteArray): Exam {
    val exam = if (name.endsWith(".txt", ignoreCase = true)) parseTxt(decodeText(bytes)).toExam() else json.decodeFromString(Exam.serializer(), bytes.decodeToString()).fixup()
    require(exam.questions.isNotEmpty()) { "no questions found" }
    return exam
}
