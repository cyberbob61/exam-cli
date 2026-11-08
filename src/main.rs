//! exam-cli: console trainer for Exam files, Pearson VUE look, training mode.
//! Spec: docs/DESIGN.md. Usage: exam-cli [exam.json]
#[allow(dead_code)] // same model file as the Exam GUI; its TXT import/export half is unused here
mod model;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use model::{decode_text, parse_txt, Exam, QType, Question};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const TEAL: Color = Color::Rgb { r: 86, g: 196, b: 160 };
const BLUE: Color = Color::Rgb { r: 122, g: 170, b: 232 };
const TEXT: Color = Color::Rgb { r: 210, g: 208, b: 200 };
const WHITE: Color = Color::Rgb { r: 245, g: 245, b: 245 };
const DIM: Color = Color::Rgb { r: 130, g: 130, b: 130 };
const PURPLE: Color = Color::Rgb { r: 60, g: 52, b: 137 };
const GREEN: Color = Color::Rgb { r: 37, g: 80, b: 11 };
const RED: Color = Color::Rgb { r: 122, g: 32, b: 32 };
const ERR: Color = Color::Rgb { r: 235, g: 100, b: 100 };
const GRAY: Color = Color::Rgb { r: 48, g: 48, b: 48 };
const MARGIN: u16 = 2;
const RULE: usize = 56;
const KEYS: [(&str, &str); 11] = [
    ("A–D, F", "choose option"),
    ("↑ / ↓", "move"),
    ("Space", "choose / toggle"),
    ("Enter", "choose / check / next"),
    ("S", "show answer"),
    ("E", "explain"),
    ("I", "open picture"),
    ("N / P", "next / prev"),
    ("R", "review all questions"),
    ("H", "hide help"),
    ("Q", "quit"),
];

/// answer state of one question, kept while you move between questions
#[derive(Clone, Default)]
struct State {
    picks: Vec<bool>,
    submitted: bool,
    show: bool,
    explain: bool,
    /// option the arrow keys are on (marked with ›)
    cursor: usize,
}

impl State {
    /// single-choice questions start with A chosen; multiple-choice start with nothing ticked
    fn new(q: &Question) -> Self {
        let mut picks = vec![false; q.choices.len()];
        if q.kind == QType::Single && !picks.is_empty() {
            picks[0] = true;
        }
        State { picks, ..Default::default() }
    }

    /// "always show answers": S already pressed and the pointer on the first right option
    /// (single-choice selects it, multiple-choice only moves the cursor there)
    fn with_answer(q: &Question) -> Self {
        let mut s = State { show: true, ..State::new(q) };
        if let Some(j) = q.choices.iter().position(|c| c.correct) {
            if q.kind == QType::Single { s.pick(q, j) } else { s.cursor = j }
        }
        s
    }

    /// choose option j (single) or flip it (multiple); the cursor follows
    fn pick(&mut self, q: &Question, j: usize) {
        if self.submitted || j >= self.picks.len() {
            return;
        }
        self.cursor = j;
        if q.kind == QType::Single {
            self.picks.iter_mut().enumerate().for_each(|(k, p)| *p = k == j);
        } else {
            self.picks[j] = !self.picks[j];
        }
    }

    /// single-choice: the selection itself moves; multiple: just the cursor
    fn arrow(&mut self, q: &Question, up: bool) {
        let n = self.picks.len();
        if self.submitted || n == 0 {
            return;
        }
        self.cursor = if up { self.cursor.saturating_sub(1) } else { (self.cursor + 1).min(n - 1) };
        if q.kind == QType::Single {
            self.pick(q, self.cursor);
        }
    }

    fn is_right(&self, q: &Question) -> bool {
        q.choices.iter().zip(&self.picks).all(|(c, &p)| c.correct == p)
    }

    /// first Enter submits; returns true when the answer was already checked, i.e. go to the next question
    fn enter(&mut self, q: &Question) -> bool {
        if self.submitted {
            return true;
        }
        // multiple-choice: Enter on a row that is not chosen yet chooses it; on a chosen row it checks
        if q.kind == QType::Multiple && self.picks.get(self.cursor) == Some(&false) {
            self.picks[self.cursor] = true;
            return false;
        }
        self.submitted = self.picks.contains(&true);
        false
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Row {
    Plain,
    Selected,
    Correct,
    Incorrect,
    /// a right option you did not choose: after a wrong answer or on S
    Answer,
}

/// the Latin key at the same place, so the keys work with a Russian layout too (the console reports "ф" for A)
fn latin(c: char) -> char {
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
    let c = c.to_lowercase().next().unwrap_or(c);
    RU.chars().position(|r| r == c).and_then(|k| EN.chars().nth(k)).unwrap_or(c.to_ascii_lowercase())
}

/// how option j is drawn (DESIGN.md "Row colors")
fn row(q: &Question, s: &State, j: usize, reveal: bool) -> Row {
    match (s.submitted && reveal, s.picks[j], q.choices[j].correct) {
        (true, true, true) => Row::Correct,
        (true, true, false) => Row::Incorrect,
        (true, false, true) => Row::Answer,
        (false, _, true) if s.show => Row::Answer,
        (false, true, _) => Row::Selected,
        _ => Row::Plain,
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

/// "›[A] text" wrapped to `room` columns of text; later lines start under the text, not under [A]
fn option_lines(mark: char, j: usize, text: &str, room: usize) -> Vec<String> {
    wrap(&text.replace('\n', " "), room.max(10))
        .into_iter()
        .enumerate()
        .map(|(k, l)| if k == 0 { format!("{mark}[{}] {l}", model::letter(j)) } else { format!("     {l}") })
        .collect()
}

/// prose is wrapped; parts between ``` lines are code and printed as is
fn text_lines(text: &str, width: usize) -> Vec<(String, bool)> {
    let mut out = vec![];
    for (k, part) in text.split("```").enumerate() {
        if k % 2 == 0 {
            let t = part.trim_matches('\n');
            if !t.is_empty() {
                out.extend(wrap(t, width).into_iter().map(|l| (l, false)));
            }
        } else {
            // a word right after the opening fence names the language; drop it
            let code = match part.split_once('\n') {
                Some((lang, rest)) if lang.trim().chars().all(char::is_alphanumeric) => rest,
                _ => part,
            };
            out.extend(code.trim_end().lines().map(|l| (format!("    {l}"), true)));
        }
    }
    out
}

struct Screen<'a, W: Write> {
    out: &'a mut W,
    y: u16,
    width: usize,
    height: u16,
    /// rows scrolled off the top with the mouse wheel
    top: u16,
}

impl<W: Write> Screen<'_, W> {
    /// one line; with a background it fills the whole row and puts `right` at the end
    fn line(&mut self, text: &str, fg: Color, bg: Option<Color>, right: &str) -> io::Result<()> {
        // nothing above or below the window, or the console scrolls and the screen turns to mush; the last row is the footer's
        let row = match self.y.checked_sub(self.top) {
            Some(r) if r + 1 < self.height => r,
            _ => {
                self.y += 1;
                return Ok(());
            }
        };
        let inner = self.width.saturating_sub(2 * MARGIN as usize);
        // a line longer than the window is cut, not wrapped: a wrapped line pushes every row below it down
        let room = inner.saturating_sub(if bg.is_some() { right.chars().count() + 1 } else { 0 });
        let text: String = text.chars().take(room).collect();
        let text = text.as_str();
        queue!(self.out, cursor::MoveTo(MARGIN, row), SetForegroundColor(fg))?;
        match bg {
            Some(bg) => {
                let pad = inner.saturating_sub(text.chars().count() + right.chars().count() + 1);
                queue!(self.out, SetBackgroundColor(bg), Print(format!("{text}{}{right} ", " ".repeat(pad))))?;
            }
            None => queue!(self.out, Print(text))?,
        }
        queue!(self.out, ResetColor)?;
        self.y += 1;
        Ok(())
    }

    /// the footer row: right after the content, but never below the window
    fn foot(&mut self) -> io::Result<()> {
        queue!(self.out, cursor::MoveTo(MARGIN, self.y.saturating_sub(self.top).min(self.height.saturating_sub(1))))
    }

    fn rule(&mut self) -> io::Result<()> {
        let n = RULE.min(self.width.saturating_sub(2 * MARGIN as usize));
        self.line(&"─".repeat(n), TEXT, None, "")
    }
}

/// "exam-cli · CCIE-ENT · always explain": the exam and the options chosen on the start screen
fn header(exam: &Exam, opts: [bool; 3]) -> String {
    let mut h = format!("exam-cli · {}", exam_id(exam));
    for (name, _) in OPTIONS.iter().zip(opts).filter(|(_, on)| *on) {
        h += &format!(" · {name}");
    }
    h
}

fn exam_id(exam: &Exam) -> &str {
    if !exam.code.trim().is_empty() && exam.code != "000-000" { &exam.code } else { &exam.title }
}

/// reveal: show CORRECT / INCORRECT after Enter (off with "don't show correct answers" until review);
/// top: rows scrolled away with the mouse wheel. Returns how many rows the whole page has.
fn render<W: Write>(out: &mut W, exam: &Exam, i: usize, s: &State, help: bool, opts: [bool; 3], reveal: bool, top: u16, (width, height): (usize, u16)) -> io::Result<u16> {
    let q = &exam.questions[i];
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width, height, top };
    sc.line(&header(exam, opts), TEAL, None, "")?;
    let kind = match q.kind {
        QType::Single => "single-choice".to_string(),
        QType::Multiple => format!("multiple-choice (choose {})", q.choices.iter().filter(|c| c.correct).count()),
    };
    sc.line(&kind, BLUE, None, "")?;
    sc.rule()?;
    for (l, code) in text_lines(&format!("Q{}. {}", i + 1, q.text), inner) {
        sc.line(&l, if code { TEXT } else { WHITE }, None, "")?;
    }
    if !q.image.is_empty() {
        sc.line("[picture]  press I to open it", BLUE, None, "")?;
    }
    sc.y += 1;
    for (j, c) in q.choices.iter().enumerate() {
        let mark = if s.cursor == j && !s.submitted { '›' } else { ' ' };
        let (fg, bg, right) = match row(q, s, j, reveal) {
            Row::Plain => (TEXT, None, ""),
            Row::Selected => (WHITE, Some(PURPLE), ""),
            Row::Correct => (WHITE, Some(GREEN), "CORRECT"),
            Row::Incorrect => (WHITE, Some(RED), "INCORRECT"),
            Row::Answer => (WHITE, Some(GREEN), ""),
        };
        // room for the text: "›[A] " in front, the label and a space behind
        let room = inner.saturating_sub(5 + right.len() + 2);
        for (k, l) in option_lines(mark, j, &c.text, room).iter().enumerate() {
            sc.line(l, fg, bg, if k == 0 { right } else { "" })?;
        }
    }
    // with "always explain" the explanation comes with the answer: once checked, or as soon as the answer is shown (S)
    if s.explain || (opts[ALWAYS_EXPLAIN] && ((s.submitted && reveal) || s.show)) {
        sc.y += 1;
        sc.line("Explanation", BLUE, None, "")?;
        let text = if q.explanation.trim().is_empty() { "(no explanation)" } else { q.explanation.as_str() };
        for (l, _) in text_lines(text, inner) {
            sc.line(&l, TEXT, None, "")?;
        }
    }
    sc.rule()?;
    if help {
        sc.line("Help", BLUE, None, "")?;
        for (k, what) in KEYS {
            sc.line(&format!("{k:<8}{what}"), TEXT, None, "")?;
        }
    } else {
        const BAR: usize = 14;
        let filled = (BAR * (i + 1) + n / 2) / n;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(BAR - filled));
        sc.foot()?;
        queue!(sc.out, SetForegroundColor(TEXT), Print(format!("Progress {bar} {}/{n}   ", i + 1)), SetForegroundColor(DIM), Print("H help"), ResetColor)?;
    }
    let rows = sc.y;
    out.flush()?;
    Ok(rows)
}

/// list of all questions with their result, shown after the last question
fn render_review<W: Write>(out: &mut W, exam: &Exam, states: &[State], opts: [bool; 3], cur: usize, (width, height): (usize, u16)) -> io::Result<()> {
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width, height, top: 0 };
    sc.line(&header(exam, opts), TEAL, None, "")?;
    sc.line("review", BLUE, None, "")?;
    sc.rule()?;
    // 7 rows go to the margin, header, rules and footer
    let visible = (height as usize).saturating_sub(7).max(1);
    let start = (cur + 1).saturating_sub(visible);
    let end = (start + visible).min(n);
    let mut wrong = 0;
    for (k, (q, s)) in exam.questions.iter().zip(states).enumerate() {
        let first = q.text.lines().find(|l| !l.trim().is_empty() && !l.starts_with("```")).unwrap_or("");
        let mark = if k == cur { '›' } else { ' ' };
        let title: String = format!("{mark}Q{}. {first}", k + 1).chars().take(inner.saturating_sub(14)).collect();
        let (bg, label) = match (s.submitted, s.is_right(q)) {
            (false, _) => (GRAY, "UNANSWERED"),
            (true, true) => (GREEN, "CORRECT"),
            (true, false) => (RED, "INCORRECT"),
        };
        // as in VCE, an unanswered question counts as incorrect
        wrong += usize::from(label != "CORRECT");
        if (start..end).contains(&k) {
            sc.line(&title, WHITE, Some(bg), label)?;
        }
    }
    sc.rule()?;
    sc.foot()?;
    queue!(sc.out, SetForegroundColor(TEXT),
        Print(format!("( {n} Questions , {wrong} Incorrect )   ")),
        SetForegroundColor(DIM),
        Print(if n > visible { format!("rows {}–{end} of {n}   ", start + 1) } else { String::new() }),
        Print("↑/↓ choose  Enter open  R/Esc back  Q quit"),
        ResetColor
    )?;
    out.flush()
}

/// writes the question's picture to %TEMP% and opens it with the default viewer
fn open_picture(q: &Question, i: usize) {
    let Some((head, data)) = q.image.split_once("base64,") else { return };
    let Ok(bytes) = B64.decode(data) else { return };
    let ext = head.trim_start_matches("data:image/").trim_end_matches(';');
    let path = std::env::temp_dir().join(format!("exam-cli-q{}.{ext}", i + 1));
    if std::fs::write(&path, bytes).is_ok() {
        let mut open = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        } else {
            std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
        };
        let _ = open.arg(&path).spawn();
    }
}

/// reads an exam: our .json, or a .txt in VCE or "Q1 / A. / Answer:" format
fn load(path: &Path) -> Result<Exam, String> {
    let mut exam = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
        let t = parse_txt(&decode_text(&fs::read(path).map_err(|e| e.to_string())?));
        Exam { title: t.title, description: t.description, questions: t.questions, ..Default::default() }
    } else {
        serde_json::from_str::<Exam>(&fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
    };
    exam.fixup();
    if exam.questions.is_empty() {
        return Err("no questions found".into());
    }
    Ok(exam)
}

/// writes name.json next to name.txt unless it is already there (the .txt is kept); returns the .json path
fn convert(txt: &Path) -> Result<PathBuf, String> {
    let json = txt.with_extension("json");
    if !json.exists() {
        let s = serde_json::to_string_pretty(&load(txt)?).map_err(|e| e.to_string())?;
        fs::write(&json, s).map_err(|e| e.to_string())?;
    }
    Ok(json)
}

/// a start screen row: a folder to go into, or an exam file with (title, questions) or the reason it cannot be read
struct Entry {
    name: String,
    path: PathBuf,
    dir: bool,
    info: Result<(String, usize), String>,
}

/// the folder above `dir`, unless `dir` is the browser's root (questions\): the browser never leaves it
fn up(dir: &Path, root: &Path) -> Option<PathBuf> {
    if dir == root { None } else { dir.parent().map(Path::to_path_buf) }
}

/// one folder of the start screen, like `ls`: `..` and the subfolders first, then the exams in it,
/// .json and the .txt that has no .json yet (it is converted when opened, not while browsing)
fn list(dir: &Path, root: &Path) -> Vec<Entry> {
    let sep = std::path::MAIN_SEPARATOR;
    let name = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let is = |p: &Path, ext: &str| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
    let folder = |name: String, path: &Path| Entry { name, path: path.to_path_buf(), dir: true, info: Ok(Default::default()) };
    let mut items: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    items.sort_by_key(|p| name(p).to_lowercase());
    let mut rows: Vec<Entry> = up(dir, root).map(|up| folder(format!("..{sep}"), &up)).into_iter().collect();
    rows.extend(items.iter().filter(|p| p.is_dir() && !name(p).starts_with('.')).map(|p| folder(format!("{}{sep}", name(p)), p)));
    rows.extend(
        items.iter()
            .filter(|p| p.is_file() && (is(p, "json") || is(p, "txt") && !p.with_extension("json").exists()))
            .map(|p| Entry { name: name(p), path: p.clone(), dir: false, info: load(p).map(|e| (e.title, e.questions.len())) }),
    );
    rows
}

/// start-screen options, indexes into `[bool; 3]`
const ALWAYS_EXPLAIN: usize = 0;
const HIDE_RESULTS: usize = 1;
const ALWAYS_SHOW: usize = 2;
const OPTIONS: [&str; 3] = ["always explain", "don't show correct answers", "always show answers"];

fn render_menu<W: Write>(out: &mut W, dir: &Path, rows: &[Entry], opts: [bool; 3], cur: usize, (width, height): (usize, u16)) -> io::Result<()> {
    queue!(out, terminal::Clear(ClearType::All))?;
    // a long folder scrolls so the › row stays in view: rows start at y 4, the options 3 rows after the last one
    let cur_y = 4 + if cur < rows.len() { cur } else { cur + 2 } as u16;
    let mut sc = Screen { out, y: 1, width, height, top: cur_y.saturating_sub(height.saturating_sub(2)) };
    sc.line("exam-cli", TEAL, None, "")?;
    sc.line(&format!("choose an exam: {}", dir.display()), BLUE, None, "")?;
    sc.rule()?;
    let w = rows.iter().filter(|r| !r.dir).map(|r| r.name.chars().count()).max().unwrap_or(0);
    for (k, r) in rows.iter().enumerate() {
        let mark = if k == cur { '›' } else { ' ' };
        match &r.info {
            _ if r.dir => sc.line(&format!("{mark}{}", r.name), if k == cur { WHITE } else { BLUE }, (k == cur).then_some(PURPLE), "")?,
            Ok((title, n)) => {
                let text = format!("{mark}{:<w$}   {title} · {n} questions", r.name);
                if k == cur { sc.line(&text, WHITE, Some(PURPLE), "")? } else { sc.line(&text, TEXT, None, "")? }
            }
            Err(e) => sc.line(&format!("{mark}{:<w$}   {e}", r.name), ERR, None, "")?,
        }
    }
    sc.rule()?;
    sc.line("Options", BLUE, None, "")?;
    for (k, name) in OPTIONS.iter().enumerate() {
        let here = cur == rows.len() + k;
        let text = format!("{}[{}] {name}", if here { '›' } else { ' ' }, if opts[k] { 'x' } else { ' ' });
        if here { sc.line(&text, WHITE, Some(PURPLE), "")? } else { sc.line(&text, TEXT, None, "")? }
    }
    sc.rule()?;
    sc.foot()?;
    queue!(sc.out, SetForegroundColor(DIM), Print("↑/↓ choose  Enter open / toggle  Backspace up  Q quit"), ResetColor)?;
    out.flush()
}

/// start screen, a folder browser over `root` and its subfolders; Some((path of the chosen exam, options)), None to quit
fn menu(root: PathBuf) -> io::Result<Option<(PathBuf, [bool; 3])>> {
    let mut out = io::stdout();
    crossterm::execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    terminal::enable_raw_mode()?;
    let _raw = Raw;
    let mut dir = root.clone();
    let mut rows = list(&dir, &root);
    // the first exam if the folder has one, else the first folder
    let mut cur = rows.iter().position(|r| !r.dir && r.info.is_ok()).unwrap_or(0);
    let mut opts = [false; 3];
    loop {
        // folder and exam rows first, then one row per option
        let n = rows.len() + OPTIONS.len();
        let (w, h) = terminal::size()?;
        // shown as questions\sub\deeper, from the root's own name down
        let shown = root.parent().and_then(|p| dir.strip_prefix(p).ok()).unwrap_or(&dir);
        render_menu(&mut out, shown, &rows, opts, cur, (w as usize, h))?;
        let key = match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => k,
            _ => continue,
        };
        // Enter on a folder goes into it; Backspace is cd .. (row 0 is .. whenever there is a parent)
        let go = match key.code {
            KeyCode::Enter if cur < rows.len() && rows[cur].dir => Some(rows[cur].path.clone()),
            KeyCode::Backspace => up(&dir, &root),
            _ => None,
        };
        if let Some(next) = go {
            let from = std::mem::replace(&mut dir, next);
            rows = list(&dir, &root);
            // going up lands on the folder you came from; a new folder starts on its first exam or folder
            cur = rows.iter().position(|r| r.path == from)
                .or_else(|| rows.iter().position(|r| !r.dir && r.info.is_ok()))
                .unwrap_or(usize::from(rows.len() > 1));
            continue;
        }
        match key.code {
            KeyCode::Up => cur = cur.saturating_sub(1),
            KeyCode::Down => cur = (cur + 1).min(n - 1),
            KeyCode::Enter | KeyCode::Char(' ') if cur >= rows.len() => opts[cur - rows.len()] ^= true,
            KeyCode::Enter if rows[cur].info.is_ok() => {
                let path = &rows[cur].path;
                if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
                    return Ok(Some((path.clone(), opts)));
                }
                match convert(path) {
                    Ok(json) => return Ok(Some((json, opts))),
                    Err(e) => rows[cur].info = Err(format!("cannot convert: {e}")),
                }
            }
            KeyCode::Esc => return Ok(None),
            KeyCode::Char(c) if latin(c) == 'q' || (latin(c) == 'c' && key.modifiers.contains(KeyModifiers::CONTROL)) => return Ok(None),
            _ => {}
        }
    }
}

/// where the start screen's folder browser starts and stops: questions\ (or samples\) next to the exe, then in the
/// current folder (`cargo run`, or macOS running a downloaded binary from a moved-away copy), else the current folder
/// itself, so the start menu with its options is always there
fn browser_root() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = exe_dir.iter().chain([&cwd]).flat_map(|base| ["questions", "samples"].map(|d| base.join(d))).find(|d| d.is_dir());
    let root = found.unwrap_or(cwd);
    std::path::absolute(&root).unwrap_or(root)
}

/// puts the terminal back even if the trainer panics
struct Raw;
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), event::DisableMouseCapture, cursor::Show, terminal::LeaveAlternateScreen);
    }
}

fn run(exam: &Exam, opts: [bool; 3]) -> io::Result<()> {
    let n = exam.questions.len();
    let mut states: Vec<State> = exam.questions.iter().map(if opts[ALWAYS_SHOW] { State::with_answer } else { State::new }).collect();
    let (mut i, mut help) = (0, false);
    // Some(row) while the review list is open
    let mut review: Option<usize> = None;
    // once review has been open, Enter on an answered question goes back to it instead of to the next question
    let mut reviewed = false;
    let mut out = io::stdout();
    // the mouse wheel scrolls the page (a long explanation) instead of reaching the app as arrow keys
    crossterm::execute!(out, terminal::EnterAlternateScreen, cursor::Hide, event::EnableMouseCapture)?;
    terminal::enable_raw_mode()?;
    let _raw = Raw;
    // Q asked "Quit?" and waits for Y / N
    let mut quitting = false;
    // page scroll and the page height it was drawn with; a new question starts at the top
    let (mut top, mut rows, mut top_of) = (0u16, 0u16, i);
    loop {
        let (width, height) = terminal::size()?;
        if top_of != i {
            (top, top_of) = (0, i);
        }
        match review {
            Some(cur) => render_review(&mut out, exam, &states, opts, cur, (width as usize, height))?,
            None => rows = render(&mut out, exam, i, &states[i], help, opts, !opts[HIDE_RESULTS] || reviewed, top, (width as usize, height))?,
        }
        if quitting {
            queue!(out, cursor::MoveTo(MARGIN, height.saturating_sub(2)), SetBackgroundColor(RED), SetForegroundColor(WHITE), Print(" Quit exam-cli?  Y yes   N no "), ResetColor)?;
            out.flush()?;
        }
        let key = match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => k,
            Event::Mouse(m) => {
                // the page shows height - 1 rows (the last row is the footer), so it scrolls until its end is in view
                let most = rows.saturating_sub(height.saturating_sub(1));
                match m.kind {
                    MouseEventKind::ScrollDown => top = (top + 3).min(most),
                    MouseEventKind::ScrollUp => top = top.saturating_sub(3),
                    _ => {}
                }
                continue;
            }
            _ => continue, // resize and key releases just redraw
        };
        // Ctrl+C asks like Q; a second Ctrl+C while asking quits
        if matches!(key.code, KeyCode::Char(c) if latin(c) == 'c' && key.modifiers.contains(KeyModifiers::CONTROL)) {
            if quitting {
                break;
            }
            quitting = true;
            continue;
        }
        if quitting {
            match key.code {
                KeyCode::Char(c) if latin(c) == 'y' => break,
                KeyCode::Char(c) if latin(c) == 'n' => quitting = false,
                KeyCode::Esc => quitting = false,
                _ => {}
            }
            continue;
        }
        if let Some(cur) = review {
            reviewed = true;
            match key.code {
                KeyCode::Up => review = Some(cur.saturating_sub(1)),
                KeyCode::Down => review = Some((cur + 1).min(n - 1)),
                KeyCode::Enter => (i, review) = (cur, None),
                KeyCode::Esc => review = None,
                KeyCode::Char(c) if latin(c) == 'r' => review = None,
                KeyCode::Char(c) if latin(c) == 'q' => quitting = true,
                _ => {}
            }
            continue;
        }
        let q = &exam.questions[i];
        let s = &mut states[i];
        match key.code {
            KeyCode::Char(c) => match latin(c) {
                // E is Explain, so option E is reached with the arrows; G and later are other commands
                'e' => s.explain = !s.explain,
                l @ ('a'..='d' | 'f') => s.pick(q, (l as u8 - b'a') as usize),
                ' ' => s.pick(q, s.cursor),
                's' => s.show = !s.show,
                'i' => open_picture(q, i),
                'n' => i = (i + 1).min(n - 1),
                'p' => i = i.saturating_sub(1),
                'h' => help = !help,
                'r' => review = Some(i),
                'q' => quitting = true,
                _ => {}
            },
            KeyCode::Enter => {
                if s.enter(q) {
                    if reviewed || i + 1 == n {
                        review = Some(i);
                    } else {
                        i += 1;
                    }
                }
            }
            KeyCode::Up => s.arrow(q, true),
            KeyCode::Down => s.arrow(q, false),
            KeyCode::Right | KeyCode::PageDown => i = (i + 1).min(n - 1),
            KeyCode::Left | KeyCode::PageUp => i = i.saturating_sub(1),
            KeyCode::Esc => quitting = true,
            _ => {}
        }
    }
    Ok(())
}

fn main() {
    let arg = std::env::args_os().nth(1);
    // double-clicked: no file on the command line, so ask for one; the console closes on exit, so errors wait for Enter
    let clicked = arg.is_none();
    let fail = |msg: String| -> ! {
        eprintln!("{msg}");
        if clicked {
            eprintln!("Press Enter to close.");
            let _ = io::stdin().read_line(&mut String::new());
        }
        std::process::exit(1);
    };
    let (path, opts) = match arg.map(PathBuf::from) {
        Some(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) => (convert(&p).unwrap_or_else(|e| fail(format!("cannot convert {}: {e}", p.display()))), [false; 3]),
        Some(p) => (p, [false; 3]),
        None => match menu(browser_root()) {
            Ok(Some(chosen)) => chosen,
            Ok(None) => return,
            Err(e) => fail(format!("exam-cli: {e}")),
        },
    };
    let exam = load(&path).unwrap_or_else(|e| fail(format!("cannot open {}: {e}", path.display())));
    if let Err(e) = run(&exam, opts) {
        fail(format!("exam-cli: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Choice;

    fn q(kind: QType, correct: &[bool]) -> Question {
        Question { kind, text: "Which?".into(), choices: correct.iter().map(|&c| Choice { text: "x".into(), correct: c }).collect(), ..Default::default() }
    }
    fn st(picks: &[bool], submitted: bool, show: bool) -> State {
        State { picks: picks.to_vec(), submitted, show, explain: false, cursor: 0 }
    }

    #[test]
    fn row_colors_follow_the_spec() {
        let one = q(QType::Single, &[false, true, false, false]);
        // picked B, not submitted: purple
        assert_eq!(row(&one, &st(&[false, true, false, false], false, false), 1, true), Row::Selected);
        // submitted right: green with CORRECT
        assert_eq!(row(&one, &st(&[false, true, false, false], true, false), 1, true), Row::Correct);
        // submitted A (wrong): A red, B green without label
        let wrong = st(&[true, false, false, false], true, false);
        assert_eq!((row(&one, &wrong, 0, true), row(&one, &wrong, 1, true), row(&one, &wrong, 2, true)), (Row::Incorrect, Row::Answer, Row::Plain));
        // "don't show correct answers": the wrong pick just stays chosen, nothing turns green or red
        assert_eq!((row(&one, &wrong, 0, false), row(&one, &wrong, 1, false)), (Row::Selected, Row::Plain));
        // S before answering: right rows green, the rest untouched
        let multi = q(QType::Multiple, &[true, true, false, true]);
        let shown = st(&[false, false, true, false], false, true);
        assert_eq!((0..4).map(|j| row(&multi, &shown, j, true)).collect::<Vec<_>>(), [Row::Answer, Row::Answer, Row::Selected, Row::Answer]);
    }

    #[test]
    fn arrows_space_and_enter() {
        let one = q(QType::Single, &[false, true, false, false]);
        let mut s = State::new(&one);
        assert_eq!((s.cursor, s.picks.clone()), (0, vec![true, false, false, false])); // A chosen by default
        s.arrow(&one, false);
        assert_eq!(s.picks, [false, true, false, false]);
        s.arrow(&one, true);
        s.arrow(&one, true); // stops at A, no jump to D
        assert_eq!((s.cursor, s.picks.clone()), (0, vec![true, false, false, false]));
        for _ in 0..5 {
            s.arrow(&one, false); // stops at D
        }
        assert_eq!((s.cursor, s.picks.clone()), (3, vec![false, false, false, true]));
        s.pick(&one, 1); // a letter moves the cursor too
        assert!(!s.enter(&one)); // first Enter checks the answer
        assert!(s.submitted && s.cursor == 1 && one.is_right(&[1]));
        s.arrow(&one, false); // locked after submit
        assert_eq!(s.cursor, 1);
        assert!(s.enter(&one)); // second Enter: next question

        let multi = q(QType::Multiple, &[true, true, false, false]);
        let mut m = State::new(&multi);
        assert!(!m.picks.contains(&true)); // nothing ticked by default
        m.arrow(&multi, false); // multiple: only the cursor moves
        assert_eq!((m.cursor, m.picks.contains(&true)), (1, false));
        assert!(!m.enter(&multi) && !m.submitted); // Enter on an unchosen row chooses it
        assert_eq!(m.picks, [false, true, false, false]);
        m.arrow(&multi, true);
        assert!(!m.enter(&multi) && !m.submitted);
        assert_eq!(m.picks, [true, true, false, false]);
        m.pick(&multi, m.cursor); // Space takes a choice back
        assert!(!m.enter(&multi) && !m.submitted && m.picks[0]);
        assert!(!m.enter(&multi) && m.submitted); // Enter on a chosen row checks
        assert_eq!(m.picks, [true, true, false, false]);
        assert!(m.enter(&multi)); // and the next Enter goes on

        // a question with no options does nothing and does not crash
        let none = q(QType::Multiple, &[]);
        let mut e = State::new(&none);
        assert!(!e.enter(&none) && !e.submitted);
    }

    #[test]
    fn always_show_answers_points_at_the_answer() {
        let s = State::with_answer(&q(QType::Single, &[false, false, true, false]));
        assert!(s.show);
        assert_eq!((s.cursor, s.picks.clone()), (2, vec![false, false, true, false]));
        // multiple-choice: the cursor goes to the first right option, nothing is ticked yet
        let m = State::with_answer(&q(QType::Multiple, &[false, true, false, true]));
        assert_eq!((m.cursor, m.picks.clone()), (1, vec![false; 4]));
    }

    #[test]
    fn long_lines_are_cut_not_wrapped() {
        let mut buf = Vec::new();
        let mut sc = Screen { out: &mut buf, y: 0, width: 20 + 2 * MARGIN as usize, height: 10, top: 0 };
        sc.line(&"x".repeat(50), TEXT, None, "").unwrap();
        sc.line(&"y".repeat(50), TEXT, Some(PURPLE), "").unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains(&"x".repeat(20)) && !s.contains(&"x".repeat(21)));
        // a highlighted row keeps one space at the end, so the text gets one less
        assert!(s.contains(&"y".repeat(19)) && !s.contains(&"y".repeat(20)));
    }

    #[test]
    fn folder_listing_and_txt_conversion() {
        let base = std::env::temp_dir().join(format!("exam-cli-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub/deeper")).unwrap();
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::write(base.join("a.txt"), "Title A\n=====\n\nQ1\nWhy?\n\nA. x\nB. y\n\nAnswer: B\n").unwrap();
        fs::write(base.join("bad.txt"), "no questions here").unwrap();
        fs::write(base.join("notes.md"), "not an exam").unwrap();
        fs::write(base.join("s.json"), r#"{"title":"S","questions":[{"text":"q","choices":[{"text":"a","correct":true}]}]}"#).unwrap();

        let s = std::path::MAIN_SEPARATOR;
        // base is the browser's root: no .. there, only in folders below it
        let got = |dir: &Path| -> Vec<(String, bool, Result<(String, usize), String>)> { list(dir, &base).into_iter().map(|r| (r.name, r.dir, r.info)).collect() };
        // folders first (hidden ones left out), then the exams; nothing is written while browsing
        assert_eq!(got(&base), [
            (format!("sub{s}"), true, Ok(Default::default())),
            ("a.txt".into(), false, Ok(("Title A".into(), 1))),
            ("bad.txt".into(), false, Err("no questions found".into())),
            ("s.json".into(), false, Ok(("S".into(), 1))),
        ]);
        assert!(!base.join("a.json").exists());
        assert_eq!(got(&base.join("sub"))[..2].iter().map(|r| r.0.clone()).collect::<Vec<_>>(), [format!("..{s}"), format!("deeper{s}")]);
        assert_eq!((up(&base, &base), up(&base.join("sub"), &base)), (None, Some(base.clone())));

        // opening a.txt writes a.json beside it; from then on the folder lists the .json instead of the .txt
        assert_eq!(convert(&base.join("a.txt")).unwrap(), base.join("a.json"));
        assert!(base.join("a.txt").is_file(), "the txt stays");
        assert_eq!(got(&base)[1].0, "a.json");
        // a json that is already there is never overwritten
        fs::write(base.join("a.json"), r#"{"title":"Edited","questions":[{"text":"q","choices":[{"text":"a","correct":true}]}]}"#).unwrap();
        convert(&base.join("a.txt")).unwrap();
        assert_eq!(got(&base)[1].2, Ok(("Edited".into(), 1)));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn keys_work_in_russian_layout() {
        let got: String = "ФИСВУАЫЧТЗРЙШНТ".chars().map(latin).collect();
        assert_eq!(got, "abcdefsxnphqiyn");
        assert_eq!((latin('B'), latin('q'), latin('7')), ('b', 'q', '7'));
    }

    #[test]
    fn start_screen_lists_options() {
        let rows = [
            Entry { name: "..\\".into(), path: PathBuf::new(), dir: true, info: Ok(Default::default()) },
            Entry { name: "demo.json".into(), path: PathBuf::new(), dir: false, info: Ok(("Demo".into(), 3)) },
        ];
        let mut buf = vec![];
        render_menu(&mut buf, Path::new("C:\\exams\\samples"), &rows, [true, false, false], 3, (80, 40)).unwrap();
        let text = plain(buf);
        for want in ["exam-cli", "choose an exam: C:\\exams\\samples", " ..\\", " demo.json   Demo · 3 questions", "Options", " [x] always explain", "›[ ] don't show correct answers", " [ ] always show answers", "Backspace up"] {
            assert!(text.contains(want), "missing {want:?} in {text:?}");
        }
        assert!(!text.contains("TRAINING MODE"));
    }

    #[test]
    fn long_options_wrap_under_their_text() {
        let lines = option_lines('›', 2, "Prompt engineering is the older term for prompt design", 20);
        assert_eq!(lines, ["›[C] Prompt engineering", "     is the older term", "     for prompt design"]);
        assert!(lines.iter().all(|l| l.chars().count() <= 5 + 20));
        assert_eq!(option_lines(' ', 0, "short", 20), [" [A] short"]);
    }

    #[test]
    fn wrapping_keeps_code_as_is() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        let lines = text_lines("Print?\n\n```python\nfor n in x:\n    y += n\n```", 40);
        assert_eq!(lines, [("Print?".into(), false), ("    for n in x:".into(), true), ("        y += n".into(), true)]);
    }

    #[test]
    fn always_explain_comes_with_shown_answers() {
        let exam = Exam { questions: vec![q(QType::Single, &[false, true])], ..Default::default() };
        let shown = |opts| {
            let mut buf = vec![];
            render(&mut buf, &exam, 0, &st(&[false, true], false, true), false, opts, true, 0, (80, 40)).unwrap();
            plain(buf).contains("Explanation")
        };
        assert!(shown([true, false, true]));
        assert!(!shown([false, false, true]));
    }

    #[test]
    fn wheel_scroll_shows_the_end_of_a_long_explanation() {
        let mut one = q(QType::Single, &[false, true]);
        one.explanation = (1..=30).map(|k| format!("line{k}")).collect::<Vec<_>>().join("\n");
        let exam = Exam { questions: vec![one], ..Default::default() };
        let draw = |top| {
            let mut buf = vec![];
            let rows = render(&mut buf, &exam, 0, &st(&[false, true], false, true), false, [true, false, true], true, top, (80, 20)).unwrap();
            (rows, plain(buf))
        };
        let (rows, first) = draw(0);
        assert!(first.contains("exam-cli") && !first.contains("line30"));
        // scrolled as far as the wheel goes: the explanation's last line is in view, the header is not
        let (_, end) = draw(rows - 19);
        assert!(end.contains("line30") && !end.contains("exam-cli"));
    }

    #[test]
    fn screen_matches_the_mockup() {
        let exam = Exam { code: "CCIE-ENT".into(), questions: vec![q(QType::Single, &[false, true, false, false]); 100], ..Default::default() };
        let mut buf = vec![];
        render(&mut buf, &exam, 36, &st(&[true, false, false, false], true, false), false, [false; 3], true, 0, (80, 40)).unwrap();
        let text = plain(buf);
        assert!(!text.contains("TRAINING MODE"));
        for want in ["exam-cli · CCIE-ENT", "single-choice", "Q37. Which?", " [A] x", "INCORRECT", "37/100", "H help"] {
            assert!(text.contains(want), "missing {want:?} in {text:?}");
        }
        assert!(!text.contains(" CORRECT"));
    }

    #[test]
    fn review_lists_results_and_score() {
        let one = q(QType::Single, &[false, true]);
        let exam = Exam { code: "DEMO".into(), questions: vec![one.clone(), one.clone(), one], ..Default::default() };
        let states = [st(&[false, true], true, false), st(&[true, false], true, false), st(&[true, false], false, false)];
        let mut buf = vec![];
        render_review(&mut buf, &exam, &states, [true, true, false], 1, (80, 40)).unwrap();
        let text = plain(buf);
        for want in ["exam-cli · DEMO · always explain · don't show correct answers", "review", " Q1. Which?", "›Q2. Which?", "CORRECT", "INCORRECT", "UNANSWERED", "( 3 Questions , 2 Incorrect )"] {
            assert!(text.contains(want), "missing {want:?} in {text:?}");
        }
    }

    #[test]
    fn long_review_scrolls_with_the_cursor() {
        let one = q(QType::Single, &[false, true]);
        let exam = Exam { code: "DEMO".into(), questions: vec![one; 6], ..Default::default() };
        let states = vec![st(&[false, true], true, false); 6];
        let mut buf = vec![];
        // height 10 leaves room for 3 rows; the cursor on Q5 shows Q3..Q5
        render_review(&mut buf, &exam, &states, [false; 3], 4, (80, 10)).unwrap();
        let text = plain(buf);
        assert!(text.contains(" Q3.") && text.contains("›Q5.") && !text.contains("Q2.") && !text.contains("Q6."), "{text:?}");
        assert!(text.contains("( 6 Questions , 0 Incorrect )") && text.contains("rows 3–5 of 6"), "{text:?}");
    }

    /// screen text without escape sequences
    fn plain(buf: Vec<u8>) -> String {
        let mut text = String::new();
        let mut esc = false;
        for ch in String::from_utf8(buf).unwrap().chars() {
            match ch {
                '\x1b' => esc = true,
                c if esc && c.is_ascii_alphabetic() => esc = false,
                _ if esc => {}
                c => text.push(c),
            }
        }
        text
    }
}
