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
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
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
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CheckBox
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.FolderOpen
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlin.math.abs

private val DARK = Color(0xFF1E1E1E)
private val YELLOW = Color(0xFFF5C518)
private val STRIP = Color(0xFFE4E4E4)
private val PAGE = Color(0xFFF2F2F2)
private val LINE = Color(0xFFBDBDBD)
private val GREEN = Color(0xFF2E7D32)
private val GREEN_BG = Color(0xFFD7F0D2)
private val RED = Color(0xFFC62828)
private val RED_BG = Color(0xFFF8D4D4)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // light status bar icons on the dark bars
        enableEdgeToEdge(statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT))
        setContent { App(viewModel()) }
    }
}

/** one run through an exam; lives in the view model so turning the phone keeps it */
class Session(val exam: Exam, val alwaysExplain: Boolean) {
    val n = exam.questions.size
    var i by mutableIntStateOf(0)
    val picks = mutableStateListOf<Set<Int>>().apply { repeat(n) { add(emptySet()) } }
    /** Show was pressed on this question: right options green, a wrong pick red */
    val shown = mutableStateListOf<Boolean>().apply { repeat(n) { add(false) } }
    val flagged = mutableStateListOf<Boolean>().apply { repeat(n) { add(false) } }
    var inReview by mutableStateOf(false)

    fun toggle(k: Int, j: Int) {
        val q = exam.questions[k]
        picks[k] = if (!q.multiple) setOf(j) else if (j in picks[k]) picks[k] - j else picks[k] + j
    }

    fun openReview() {
        inReview = true
    }

    // a chosen answer counts as given; Show only reveals the right one
    fun isRight(k: Int) = picks[k].isNotEmpty() && exam.questions[k].isRight(picks[k])
}

class AppModel(app: Application) : AndroidViewModel(app) {
    private val library = Library(app)
    private val prefs = app.getSharedPreferences("exam", Context.MODE_PRIVATE)
    var entries by mutableStateOf(library.scan())
    var alwaysExplain by mutableStateOf(prefs.getBoolean("alwaysExplain", false))
        private set
    var session by mutableStateOf<Session?>(null)
    var message by mutableStateOf<String?>(null)

    fun saveAlwaysExplain(on: Boolean) {
        alwaysExplain = on
        prefs.edit().putBoolean("alwaysExplain", on).apply()
    }

    fun import(uri: Uri) {
        message = library.import(uri)
        entries = library.scan()
    }

    fun delete(e: Entry) {
        library.delete(e)
        entries = library.scan()
    }

    fun start(e: Entry) {
        e.exam?.let { session = Session(it, alwaysExplain) }
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
    var toDelete by remember { mutableStateOf<Entry?>(null) }
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
            items(m.entries) { e -> ExamCard(e, onPlay = { m.start(e) }, onDelete = { toDelete = e }) }
            item { OptionsCard(m) }
            item { Text("Long-press an exam to delete it.", color = Color.Gray, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 4.dp)) }
        }
    }
    toDelete?.let { e ->
        Confirm(
            "Delete Exam",
            "Delete ${e.exam?.title ?: e.file.name}?

It is removed from the app; the file you opened it from (e.g. in Downloads) stays.",
            onYes = {
                m.delete(e)
                toDelete = null
            },
            onNo = { toDelete = null },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ExamCard(e: Entry, onPlay: () -> Unit, onDelete: () -> Unit) {
    val ctx = LocalContext.current
    Row(
        Modifier.fillMaxWidth()
            .background(Color.White, RoundedCornerShape(6.dp))
            .border(1.dp, LINE, RoundedCornerShape(6.dp))
            .combinedClickable(onClick = { if (e.exam != null) onPlay() }, onLongClick = onDelete)
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
        Row(Modifier.fillMaxWidth().clickable { m.saveAlwaysExplain(!m.alwaysExplain) }, verticalAlignment = Alignment.CenterVertically) {
            Text("Always explain (with Check)", Modifier.weight(1f))
            Switch(checked = m.alwaysExplain, onCheckedChange = { m.saveAlwaysExplain(it) })
        }
    }
}

// ---------- one question ----------

@Composable
fun QuestionScreen(m: AppModel, s: Session) {
    var quitting by remember { mutableStateOf(false) }
    BackHandler { quitting = true }
    // one page per question plus a last one: swiping past the last question opens the review
    val pager = rememberPagerState(initialPage = s.i) { s.n + 1 }
    // swiping changes the question; picking one in the review moves the pager there
    LaunchedEffect(pager) {
        snapshotFlow { pager.currentPage }.collect {
            if (it == s.n) {
                s.i = s.n - 1
                s.openReview()
            } else {
                s.i = it
            }
        }
    }
    LaunchedEffect(s.i) { if (pager.currentPage != s.i) pager.scrollToPage(s.i) }
    Scaffold(
        topBar = {
            Column {
                Column(Modifier.fillMaxWidth().background(DARK).statusBarsPadding().padding(vertical = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(s.exam.id, color = Color.White, fontSize = 20.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = 12.dp))
                    Spacer(Modifier.height(4.dp))
                    ProgressLine(s.i, s.n)
                }
                CheckStrip(s)
            }
        },
    ) { pad ->
        HorizontalPager(pager, Modifier.padding(pad).fillMaxSize().background(Color.White)) { k ->
            if (k == s.n) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Review", fontSize = 20.sp, color = Color.Gray) }
                return@HorizontalPager
            }
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(14.dp)) {
                QuestionBody(s.exam.questions[k], s, k)
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

/** "Progress ████░░░░ 3/53", the same bar as exam-cli */
@Composable
fun ProgressLine(i: Int, n: Int) {
    val cells = 24
    val filled = (cells * (i + 1) + n / 2) / n
    Text(
        "Progress ${"█".repeat(filled)}${"░".repeat(cells - filled)} ${i + 1}/$n",
        color = Color(0xFFDDDDDD),
        fontFamily = FontFamily.Monospace,
        fontSize = 13.sp,
        maxLines = 1,
        modifier = Modifier.background(Color(0xFF2C2C2C)).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

/** grey strip: Check in the middle shows the right answer (press again to hide it), the star flags the question */
@Composable
fun CheckStrip(s: Session) {
    Box(Modifier.fillMaxWidth().background(STRIP).padding(vertical = 8.dp)) {
        val on = s.shown[s.i]
        Text(
            if (on) "Hide" else "Check",
            fontSize = 22.sp,
            modifier = Modifier.align(Alignment.Center).background(Color.White).border(2.dp, Color.Black).clickable { s.shown[s.i] = !on }.padding(horizontal = 40.dp, vertical = 6.dp),
        )
        IconButton(onClick = { s.flagged[s.i] = !s.flagged[s.i] }, modifier = Modifier.align(Alignment.CenterEnd)) {
            if (s.flagged[s.i]) Icon(Icons.Filled.Star, "Flagged", tint = Color(0xFFE6A100)) else Icon(Icons.Outlined.StarBorder, "Flag for review", tint = Color.Gray)
        }
    }
}

@Composable
fun QuestionBody(q: Question, s: Session, k: Int) {
    val shown = s.shown[k]
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
        val picked = j in s.picks[k]
        val (bg, label) = when {
            shown && picked && c.correct -> GREEN_BG to "CORRECT"
            shown && picked -> RED_BG to "INCORRECT"
            shown && c.correct -> GREEN_BG to ""
            else -> Color.Transparent to ""
        }
        Row(
            Modifier.fillMaxWidth().padding(vertical = 3.dp).background(bg, RoundedCornerShape(6.dp)).clickable { s.toggle(k, j) }.padding(vertical = 6.dp, horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (q.multiple) Checkbox(picked, null) else RadioButton(picked, null)
            Text("${letter(j)}. ${c.text}", Modifier.weight(1f).padding(start = 4.dp), fontSize = 16.sp)
            if (label.isNotEmpty()) Text(label, color = if (label == "CORRECT") GREEN else RED, fontWeight = FontWeight.Bold, fontSize = 12.sp)
        }
    }
    if (s.alwaysExplain && shown) {
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
        // a sideways swipe goes back to the last question, like paging back from the review
        val swipeBack = Modifier.pointerInput(s) {
            var dx = 0f
            detectHorizontalDragGestures(
                onDragStart = { dx = 0f },
                onDragEnd = {
                    if (abs(dx) > 100.dp.toPx()) {
                        s.i = s.n - 1
                        s.inReview = false
                    }
                },
                onHorizontalDrag = { _, d -> dx += d },
            )
        }
        LazyColumn(Modifier.padding(pad).fillMaxSize().background(Color.White).then(swipeBack)) {
            itemsIndexed(s.exam.questions) { k, q ->
                val (color, label) = when {
                    s.picks[k].isEmpty() -> Color.Gray to "UNANSWERED"
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
