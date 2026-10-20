// Exam for Android: VCE-player style list of exams and exam screens, same exam files as exam-cli.
package com.cyberbob61.exam

import android.app.Application
import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import android.text.format.Formatter
import android.util.Base64
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Undo
import androidx.compose.material.icons.automirrored.outlined.Assignment
import androidx.compose.material.icons.filled.CheckBox
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.FastForward
import androidx.compose.material.icons.filled.FastRewind
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.PlayCircle
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.delay

private val DARK = Color(0xFF1E1E1E)
private val YELLOW = Color(0xFFF5C518)
private val STRIP = Color(0xFFE4E4E4)
private val PAGE = Color(0xFFF2F2F2)
private val LINE = Color(0xFFBDBDBD)
private val GREEN = Color(0xFF2E7D32)
private val GREEN_BG = Color(0xFFD7F0D2)
private val RED = Color(0xFFC62828)
private val RED_BG = Color(0xFFF8D4D4)
private val PICKED_BG = Color(0xFFDDE7FF)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // light status bar icons on the dark bars
        enableEdgeToEdge(statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT))
        setContent { App(viewModel()) }
    }
}

/** one run through an exam; lives in the view model so turning the phone keeps it */
class Session(val exam: Exam, val alwaysExplain: Boolean, val hideResults: Boolean) {
    val n = exam.questions.size
    var i by mutableIntStateOf(0)
    val picks = mutableStateListOf<Set<Int>>().apply { repeat(n) { add(emptySet()) } }
    val checked = mutableStateListOf<Boolean>().apply { repeat(n) { add(false) } }
    val flagged = mutableStateListOf<Boolean>().apply { repeat(n) { add(false) } }
    val explain = mutableStateListOf<Boolean>().apply { repeat(n) { add(false) } }
    var inReview by mutableStateOf(false)
    /** after the first review, results show everywhere even with "don't show correct answers" */
    var reviewed by mutableStateOf(false)
    var timeUp by mutableStateOf(false)
    val endsAt = if (exam.timeLimit > 0) System.currentTimeMillis() + exam.timeLimit * 60_000L else 0L

    val reveal get() = !hideResults || reviewed

    fun toggle(j: Int) {
        if (checked[i]) return
        val q = exam.questions[i]
        picks[i] = if (!q.multiple) setOf(j) else if (j in picks[i]) picks[i] - j else picks[i] + j
    }

    fun check() {
        if (picks[i].isNotEmpty()) checked[i] = true
    }

    fun openReview() {
        reviewed = true
        inReview = true
    }

    fun isRight(k: Int) = checked[k] && exam.questions[k].isRight(picks[k])
}

class AppModel(app: Application) : AndroidViewModel(app) {
    private val library = Library(app)
    private val prefs = app.getSharedPreferences("exam", Context.MODE_PRIVATE)
    var entries by mutableStateOf(library.scan())
    var alwaysExplain by mutableStateOf(prefs.getBoolean("alwaysExplain", false))
        private set
    var hideResults by mutableStateOf(prefs.getBoolean("hideResults", false))
        private set
    var session by mutableStateOf<Session?>(null)
    var message by mutableStateOf<String?>(null)

    fun setOption(key: String, on: Boolean) {
        if (key == "alwaysExplain") alwaysExplain = on else hideResults = on
        prefs.edit().putBoolean(key, on).apply()
    }

    fun import(uri: Uri) {
        message = library.import(uri)
        entries = library.scan()
    }

    fun start(e: Entry) {
        e.exam?.let { session = Session(it, alwaysExplain, hideResults) }
    }
}

@Composable
fun App(m: AppModel) {
    MaterialTheme(colorScheme = lightColorScheme(primary = DARK, secondary = YELLOW)) {
        val s = m.session
        when {
            s == null -> ListScreen(m)
            s.inReview -> ReviewScreen(m, s)
            else -> QuestionScreen(m, s)
        }
        m.message?.let { msg ->
            AlertDialog(
                onDismissRequest = { m.message = null },
                confirmButton = { TextButton(onClick = { m.message = null }) { Text("OK") } },
                text = { Text(msg) },
            )
        }
    }
}

/** the dark bar on top of every screen */
@Composable
fun DarkBar(title: String, sub: String?, onBack: (() -> Unit)?, actions: @Composable RowScope.() -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().background(DARK).statusBarsPadding().height(60.dp).padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = Color.White) }
        } else {
            Spacer(Modifier.width(12.dp))
        }
        Icon(Icons.Filled.CheckBox, null, tint = YELLOW, modifier = Modifier.size(30.dp))
        Column(Modifier.weight(1f).padding(start = 8.dp)) {
            Text(title, color = Color.White, fontSize = 19.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (sub != null) Text(sub, color = Color(0xFFDDDDDD), fontSize = 13.sp)
        }
        actions()
    }
}

// ---------- exam list ----------

@Composable
fun ListScreen(m: AppModel) {
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> if (uri != null) m.import(uri) }
    Scaffold(
        topBar = {
            DarkBar("Exam", "${m.entries.count { it.exam != null }} exams", onBack = null) {
                TextButton(onClick = { pick.launch(arrayOf("*/*")) }) {
                    Icon(Icons.Outlined.FolderOpen, null, tint = YELLOW)
                    Text(" Open file", color = Color.White)
                }
            }
        },
    ) { pad ->
        LazyColumn(
            Modifier.padding(pad).fillMaxSize().background(PAGE),
            contentPadding = PaddingValues(10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(m.entries) { e -> ExamCard(e) { m.start(e) } }
            item { OptionsCard(m) }
        }
    }
}

@Composable
fun ExamCard(e: Entry, onPlay: () -> Unit) {
    val ctx = LocalContext.current
    Row(
        Modifier.fillMaxWidth()
            .background(Color.White, RoundedCornerShape(6.dp))
            .border(1.dp, LINE, RoundedCornerShape(6.dp))
            .clickable(enabled = e.exam != null, onClick = onPlay)
            .padding(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(
            Modifier.size(width = 44.dp, height = 54.dp).border(2.dp, DARK, RoundedCornerShape(4.dp)),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Icon(Icons.Outlined.Description, null, tint = DARK, modifier = Modifier.size(28.dp))
            Text("EXAM", fontSize = 8.sp, fontWeight = FontWeight.Bold)
        }
        Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
            Text(e.exam?.let { "${it.code} - ${it.title}" } ?: e.file.name, fontWeight = FontWeight.Bold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("File: ${e.file.name}", fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (e.error != null) {
                Text(e.error, fontSize = 14.sp, color = RED)
            } else {
                Text("Size: ${Formatter.formatShortFileSize(ctx, e.file.length())} · ${e.exam!!.questions.size} questions", fontSize = 14.sp)
            }
        }
        if (e.exam != null) {
            Box(Modifier.size(54.dp).background(Color(0xFFDDDDDD), RoundedCornerShape(4.dp)), contentAlignment = Alignment.Center) {
                Icon(Icons.Outlined.PlayCircle, "Start", tint = Color(0xFF555555), modifier = Modifier.size(36.dp))
            }
        }
    }
}

@Composable
fun OptionsCard(m: AppModel) {
    Column(Modifier.fillMaxWidth().background(Color.White, RoundedCornerShape(6.dp)).border(1.dp, LINE, RoundedCornerShape(6.dp)).padding(12.dp)) {
        Text("Options", fontWeight = FontWeight.Bold)
        for ((key, label, on) in listOf(Triple("alwaysExplain", "Always explain", m.alwaysExplain), Triple("hideResults", "Don't show correct answers", m.hideResults))) {
            Row(Modifier.fillMaxWidth().clickable { m.setOption(key, !on) }, verticalAlignment = Alignment.CenterVertically) {
                Text(label, Modifier.weight(1f))
                Switch(checked = on, onCheckedChange = { m.setOption(key, it) })
            }
        }
    }
}

// ---------- one question ----------

private fun clock(ms: Long): String {
    val s = ms.coerceAtLeast(0) / 1000
    return "%02d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60)
}

@Composable
fun QuestionScreen(m: AppModel, s: Session) {
    var quitting by remember { mutableStateOf(false) }
    BackHandler { quitting = true }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    if (s.endsAt > 0) {
        LaunchedEffect(s) {
            while (true) {
                now = System.currentTimeMillis()
                if (now >= s.endsAt) {
                    s.timeUp = true
                    s.openReview()
                    break
                }
                delay(1000)
            }
        }
    }
    val q = s.exam.questions[s.i]
    Scaffold(
        topBar = {
            Column {
                val sub = if (s.endsAt > 0) "Time Left: ${clock(s.endsAt - now)}" else "${s.n} questions"
                DarkBar(s.exam.id, sub, onBack = { quitting = true }) {
                    IconButton(onClick = { s.check() }) { Icon(Icons.Filled.DoneAll, "Check answer", tint = Color.White) }
                    IconButton(onClick = { s.openReview() }) { Icon(Icons.AutoMirrored.Outlined.Assignment, "Review", tint = Color.White) }
                }
                ItemStrip(s)
            }
        },
        bottomBar = { NavBar(s) },
    ) { pad ->
        key(s.i) {
            Column(Modifier.padding(pad).fillMaxSize().background(Color.White).verticalScroll(rememberScrollState()).padding(14.dp)) {
                QuestionBody(q, s)
            }
        }
    }
    if (quitting) {
        Confirm(
            "Quit Exam",
            "You have chosen to quit the exam. Your answers will NOT be scored.\n\nAre you sure you want to quit?",
            onYes = { m.session = null },
            onNo = { quitting = false },
        )
    }
}

/** the grey strip with the yellow "Item N of M" pill and the explain / reset / flag buttons */
@Composable
fun ItemStrip(s: Session) {
    Row(Modifier.fillMaxWidth().background(STRIP).padding(horizontal = 6.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(
            Modifier.background(Brush.verticalGradient(listOf(Color(0xFFFFE27A), YELLOW)), RoundedCornerShape(20.dp)).padding(horizontal = 10.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Outlined.Description, null, tint = DARK, modifier = Modifier.size(22.dp))
            Column(Modifier.padding(start = 6.dp)) {
                Text("Question ${s.i + 1}", fontWeight = FontWeight.Bold, fontSize = 16.sp)
                Text("Item ${s.i + 1} of ${s.n}", fontSize = 12.sp)
            }
        }
        Spacer(Modifier.weight(1f))
        IconButton(onClick = { s.explain[s.i] = !s.explain[s.i] }) { Icon(Icons.Outlined.Info, "Explain", tint = if (s.explain[s.i]) DARK else Color.Gray) }
        IconButton(onClick = { if (!s.checked[s.i]) s.picks[s.i] = emptySet() }) { Icon(Icons.AutoMirrored.Filled.Undo, "Reset answer", tint = Color.Gray) }
        IconButton(onClick = { s.flagged[s.i] = !s.flagged[s.i] }) {
            if (s.flagged[s.i]) Icon(Icons.Filled.Star, "Flagged", tint = Color(0xFFE6A100)) else Icon(Icons.Outlined.StarBorder, "Flag for review", tint = Color.Gray)
        }
    }
}

@Composable
fun QuestionBody(q: Question, s: Session) {
    val reveal = s.reveal
    val checked = s.checked[s.i]
    QuestionText(q.text)
    val bmp = remember(q.image) {
        q.image.substringAfter("base64,", "").takeIf { it.isNotEmpty() }?.let {
            runCatching { Base64.decode(it, Base64.DEFAULT).let { b -> BitmapFactory.decodeByteArray(b, 0, b.size)?.asImageBitmap() } }.getOrNull()
        }
    }
    if (bmp != null) Image(bmp, null, Modifier.fillMaxWidth().padding(vertical = 8.dp), contentScale = ContentScale.FillWidth)
    if (q.multiple) Text("Choose ${q.choices.count { it.correct }}.", color = Color.Gray, modifier = Modifier.padding(top = 6.dp))
    Spacer(Modifier.height(8.dp))
    q.choices.forEachIndexed { j, c ->
        val picked = j in s.picks[s.i]
        val shown = checked && reveal
        val (bg, label) = when {
            shown && picked && c.correct -> GREEN_BG to "CORRECT"
            shown && picked -> RED_BG to "INCORRECT"
            shown && c.correct -> GREEN_BG to ""
            picked -> PICKED_BG to ""
            else -> Color.Transparent to ""
        }
        Row(
            Modifier.fillMaxWidth().padding(vertical = 3.dp).background(bg, RoundedCornerShape(6.dp)).clickable(enabled = !checked) { s.toggle(j) }.padding(vertical = 6.dp, horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (q.multiple) Checkbox(picked, null) else RadioButton(picked, null)
            Text("${letter(j)}. ${c.text}", Modifier.weight(1f).padding(start = 4.dp), fontSize = 16.sp)
            if (label.isNotEmpty()) Text(label, color = if (label == "CORRECT") GREEN else RED, fontWeight = FontWeight.Bold, fontSize = 12.sp)
        }
    }
    if (checked && !reveal) Text("Answer saved. Results are shown in the review.", color = Color.Gray, modifier = Modifier.padding(top = 10.dp))
    if (s.explain[s.i] || (s.alwaysExplain && checked && reveal)) {
        Column(Modifier.fillMaxWidth().padding(top = 14.dp).background(PAGE, RoundedCornerShape(6.dp)).border(1.dp, LINE, RoundedCornerShape(6.dp)).padding(12.dp)) {
            Text("Explanation", fontWeight = FontWeight.Bold)
            Text(q.explanation.ifBlank { "(no explanation)" })
        }
    }
}

/** question text; parts between ``` lines are code, shown in a monospace box */
@Composable
fun QuestionText(text: String) {
    text.split("```").forEachIndexed { k, part ->
        if (k % 2 == 0) {
            val t = part.trim('\n')
            if (t.isNotEmpty()) Text(t, fontSize = 17.sp, lineHeight = 24.sp)
        } else {
            // a word right after the opening fence names the language; drop it
            val nl = part.indexOf('\n')
            val code = if (nl >= 0 && part.substring(0, nl).trim().all { it.isLetterOrDigit() }) part.substring(nl + 1) else part
            Text(
                code.trimEnd(),
                fontFamily = FontFamily.Monospace,
                fontSize = 14.sp,
                modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp).background(PAGE).border(1.dp, LINE).padding(10.dp).horizontalScroll(rememberScrollState()),
            )
        }
    }
}

@Composable
fun NavBar(s: Session) {
    Column(Modifier.background(Color.White).navigationBarsPadding()) {
        HorizontalDivider(color = Color(0xFF29B6F6))
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 2.dp), horizontalArrangement = Arrangement.End) {
            val nav = @Composable { icon: ImageVector, what: String, to: Int ->
                IconButton(onClick = { s.i = to }, enabled = to != s.i) { Icon(icon, what) }
            }
            nav(Icons.Filled.SkipPrevious, "First", 0)
            nav(Icons.Filled.FastRewind, "Previous", (s.i - 1).coerceAtLeast(0))
            nav(Icons.Filled.FastForward, "Next", (s.i + 1).coerceAtMost(s.n - 1))
            nav(Icons.Filled.SkipNext, "Last", s.n - 1)
        }
    }
}

// ---------- review ----------

@Composable
fun ReviewScreen(m: AppModel, s: Session) {
    var ending by remember { mutableStateOf(false) }
    BackHandler { s.inReview = false }
    val wrong = (0 until s.n).count { !s.isRight(it) } // as in VCE, unanswered counts as incorrect
    Scaffold(
        topBar = { DarkBar("Review", s.exam.id, onBack = { s.inReview = false }) },
        bottomBar = {
            Row(Modifier.fillMaxWidth().background(DARK).navigationBarsPadding().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("( ${s.n} Questions , $wrong Incorrect )", color = Color.White, modifier = Modifier.weight(1f))
                Button(onClick = { ending = true }, colors = ButtonDefaults.buttonColors(containerColor = YELLOW, contentColor = DARK)) { Text("End Exam") }
            }
        },
    ) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize().background(Color.White)) {
            if (s.timeUp) item { Text("Time is up.", color = RED, fontWeight = FontWeight.Bold, modifier = Modifier.padding(12.dp)) }
            itemsIndexed(s.exam.questions) { k, q ->
                val (color, label) = when {
                    !s.checked[k] -> Color.Gray to "UNANSWERED"
                    s.isRight(k) -> GREEN to "CORRECT"
                    else -> RED to "INCORRECT"
                }
                Row(
                    Modifier.fillMaxWidth().clickable { s.i = k; s.inReview = false }.padding(horizontal = 12.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (s.flagged[k]) Icon(Icons.Filled.Star, "Flagged", tint = Color(0xFFE6A100), modifier = Modifier.size(18.dp)) else Spacer(Modifier.width(18.dp))
                    val first = q.text.lineSequence().firstOrNull { it.isNotBlank() && !it.startsWith("```") }.orEmpty()
                    Text("Q${k + 1}. $first", Modifier.weight(1f).padding(horizontal = 8.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(label, color = color, fontWeight = FontWeight.Bold, fontSize = 12.sp)
                }
                HorizontalDivider(color = Color(0xFFE0E0E0))
            }
        }
    }
    if (ending) {
        Confirm(
            "End Exam",
            "You have chosen to end the exam. If you click Yes, you will NOT be able to return to it.\n\nAre you sure you want to end the exam?",
            onYes = { m.session = null },
            onNo = { ending = false },
        )
    }
}

/** dark Yes / No question, like the VCE player's */
@Composable
fun Confirm(title: String, text: String, onYes: () -> Unit, onNo: () -> Unit) {
    AlertDialog(
        onDismissRequest = onNo,
        containerColor = DARK,
        titleContentColor = Color.White,
        textContentColor = Color(0xFFEEEEEE),
        title = { Text(title) },
        text = { Text(text) },
        confirmButton = { TextButton(onClick = onYes) { Text("Yes", color = YELLOW) } },
        dismissButton = { TextButton(onClick = onNo) { Text("No", color = YELLOW) } },
    )
}
