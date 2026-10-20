//! exam-cli: console trainer for Exam files, Pearson VUE look, training mode.
//! Spec: docs/DESIGN.md. Usage: exam-cli [exam.json]
#[allow(dead_code)] // same model file as the Exam GUI; its TXT import/export half is unused here
mod model;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
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
    ("Enter", "submit"),
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
    fn enter(&mut self) -> bool {
        if self.submitted {
            return true;
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
}

impl<W: Write> Screen<'_, W> {
    /// one line; with a background it fills the whole row and puts `right` at the end
    fn line(&mut self, text: &str, fg: Color, bg: Option<Color>, right: &str) -> io::Result<()> {
        // nothing below the window, or the console scrolls and the screen turns to mush; the last row is the footer's
        if self.y + 1 >= self.height {
            self.y += 1;
            return Ok(());
        }
        let inner = self.width.saturating_sub(2 * MARGIN as usize);
        queue!(self.out, cursor::MoveTo(MARGIN, self.y), SetForegroundColor(fg))?;
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
        queue!(self.out, cursor::MoveTo(MARGIN, self.y.min(self.height.saturating_sub(1))))
    }

    fn rule(&mut self) -> io::Result<()> {
        let n = RULE.min(self.width.saturating_sub(2 * MARGIN as usize));
        self.line(&"─".repeat(n), TEXT, None, "")
    }
}

/// "exam-cli · CCIE-ENT · always explain": the exam and the options chosen on the start screen
fn header(exam: &Exam, opts: [bool; 2]) -> String {
    let mut h = format!("exam-cli · {}", exam_id(exam));
    for (name, _) in OPTIONS.iter().zip(opts).filter(|(_, on)| *on) {
        h += &format!(" · {name}");
    }
    h
}

fn exam_id(exam: &Exam) -> &str {
    if !exam.code.trim().is_empty() && exam.code != "000-000" { &exam.code } else { &exam.title }
}

// shortcut: lines past the bottom of the terminal are cut off; add scrolling if questions get that long
/// reveal: show CORRECT / INCORRECT after Enter (off with "don't show correct answers" until review)
fn render<W: Write>(out: &mut W, exam: &Exam, i: usize, s: &State, help: bool, opts: [bool; 2], reveal: bool, (width, height): (usize, u16)) -> io::Result<()> {
    let q = &exam.questions[i];
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width, height };
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
    if s.explain || (opts[ALWAYS_EXPLAIN] && s.submitted && reveal) {
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
    out.flush()
}

/// list of all questions with their result, shown after the last question
fn render_review<W: Write>(out: &mut W, exam: &Exam, states: &[State], opts: [bool; 2], cur: usize, (width, height): (usize, u16)) -> io::Result<()> {
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width, height };
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
        let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&path).spawn();
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

/// a start screen row: an exam file with (title, questions), or a .txt that could not be converted
struct Entry {
    name: String,
    path: PathBuf,
    info: Result<(String, usize), String>,
}

/// converts new .txt files in samples\ and questions\, then lists the .json exams there
fn scan(base: &Path) -> Vec<Entry> {
    let mut rows = vec![];
    for dir in ["samples", "questions"] {
        let files = |ext: &str| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = fs::read_dir(base.join(dir)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))).collect();
            v.sort();
            v
        };
        let name = |p: &Path| format!("{dir}\\{}", p.file_name().unwrap_or_default().to_string_lossy());
        let failed: Vec<Entry> = files("txt").into_iter().filter_map(|t| convert(&t).err().map(|e| Entry { name: name(&t), info: Err(format!("cannot convert: {e}")), path: t })).collect();
        rows.extend(files("json").into_iter().map(|j| Entry { name: name(&j), info: load(&j).map(|e| (e.title, e.questions.len())), path: j }));
        rows.extend(failed);
    }
    rows
}

/// start-screen options, indexes into `[bool; 2]`
const ALWAYS_EXPLAIN: usize = 0;
const HIDE_RESULTS: usize = 1;
const OPTIONS: [&str; 2] = ["always explain", "don't show correct answers"];

fn render_menu<W: Write>(out: &mut W, rows: &[Entry], opts: [bool; 2], cur: usize, (width, height): (usize, u16)) -> io::Result<()> {
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width, height };
    sc.line("exam-cli", TEAL, None, "")?;
    sc.line("choose an exam", BLUE, None, "")?;
    sc.rule()?;
    let w = rows.iter().map(|r| r.name.chars().count()).max().unwrap_or(0);
    for (k, r) in rows.iter().enumerate() {
        let mark = if k == cur { '›' } else { ' ' };
        match &r.info {
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
    queue!(sc.out, SetForegroundColor(DIM), Print("↑/↓ choose  Enter open / toggle  Q quit"), ResetColor)?;
    out.flush()
}

/// start screen; Some((path of the chosen exam, options)), None to quit
fn menu(rows: &[Entry]) -> io::Result<Option<(PathBuf, [bool; 2])>> {
    let mut out = io::stdout();
    crossterm::execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    terminal::enable_raw_mode()?;
    let _raw = Raw;
    // exam rows first, then one row per option
    let n = rows.len() + OPTIONS.len();
    let mut cur = rows.iter().position(|r| r.info.is_ok()).unwrap_or(0);
    let mut opts = [false; 2];
    loop {
        let (w, h) = terminal::size()?;
        render_menu(&mut out, rows, opts, cur, (w as usize, h))?;
        let key = match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => k,
            _ => continue,
        };
        match key.code {
            KeyCode::Up => cur = cur.saturating_sub(1),
            KeyCode::Down => cur = (cur + 1).min(n - 1),
            KeyCode::Enter | KeyCode::Char(' ') if cur >= rows.len() => opts[cur - rows.len()] ^= true,
            KeyCode::Enter if rows[cur].info.is_ok() => return Ok(Some((rows[cur].path.clone(), opts))),
            KeyCode::Esc => return Ok(None),
            KeyCode::Char(c) if latin(c) == 'q' || (latin(c) == 'c' && key.modifiers.contains(KeyModifiers::CONTROL)) => return Ok(None),
            _ => {}
        }
    }
}

/// puts the terminal back even if the trainer panics
struct Raw;
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
    }
}

fn run(exam: &Exam, opts: [bool; 2]) -> io::Result<()> {
    let n = exam.questions.len();
    let mut states: Vec<State> = exam.questions.iter().map(State::new).collect();
    let (mut i, mut help) = (0, false);
    // Some(row) while the review list is open
    let mut review: Option<usize> = None;
    // once review has been open, Enter on an answered question goes back to it instead of to the next question
    let mut reviewed = false;
    let mut out = io::stdout();
    crossterm::execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    terminal::enable_raw_mode()?;
    let _raw = Raw;
    // Q asked "Quit?" and waits for Y / N
    let mut quitting = false;
    loop {
        let (width, height) = terminal::size()?;
        match review {
            Some(cur) => render_review(&mut out, exam, &states, opts, cur, (width as usize, height))?,
            None => render(&mut out, exam, i, &states[i], help, opts, !opts[HIDE_RESULTS] || reviewed, (width as usize, height))?,
        }
        if quitting {
            queue!(out, cursor::MoveTo(MARGIN, height.saturating_sub(2)), SetBackgroundColor(RED), SetForegroundColor(WHITE), Print(" Quit exam-cli?  Y yes   N no "), ResetColor)?;
            out.flush()?;
        }
        let key = match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => k,
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
                if s.enter() {
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
    // samples\ and questions\ live next to the exe; under `cargo run` they are in the current folder
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
    let base = if exe_dir.join("samples").is_dir() || exe_dir.join("questions").is_dir() { exe_dir } else { PathBuf::from(".") };
    let (path, opts) = match arg.map(PathBuf::from) {
        Some(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) => (convert(&p).unwrap_or_else(|e| fail(format!("cannot convert {}: {e}", p.display()))), [false; 2]),
        Some(p) => (p, [false; 2]),
        None => {
            let rows = scan(&base);
            if rows.is_empty() {
                let p = match rfd::FileDialog::new().set_title("Open exam").add_filter("Exam", &["json", "txt"]).pick_file() {
                    Some(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) => convert(&p).unwrap_or_else(|e| fail(format!("cannot convert {}: {e}", p.display()))),
                    Some(p) => p,
                    None => return,
                };
                (p, [false; 2])
            } else {
                match menu(&rows) {
                    Ok(Some(chosen)) => chosen,
                    Ok(None) => return,
                    Err(e) => fail(format!("exam-cli: {e}")),
                }
            }
        }
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
        assert!(!s.enter()); // first Enter checks the answer
        assert!(s.submitted && s.cursor == 1 && one.is_right(&[1]));
        s.arrow(&one, false); // locked after submit
        assert_eq!(s.cursor, 1);
        assert!(s.enter()); // second Enter: next question

        let multi = q(QType::Multiple, &[true, true, false, false]);
        let mut m = State::new(&multi);
        assert!(!m.picks.contains(&true)); // nothing ticked by default
        m.arrow(&multi, false); // multiple: only the cursor moves
        assert_eq!((m.cursor, m.picks.contains(&true)), (1, false));
        assert!(!m.enter() && !m.submitted); // nothing ticked: Enter neither checks nor moves on
        m.pick(&multi, m.cursor); // Space
        m.arrow(&multi, true);
        m.pick(&multi, m.cursor);
        m.enter();
        assert_eq!((m.submitted, m.picks.clone()), (true, vec![true, true, false, false]));
    }

    #[test]
    fn scan_converts_txt_once() {
        let base = std::env::temp_dir().join(format!("exam-cli-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("questions")).unwrap();
        fs::create_dir_all(base.join("samples")).unwrap();
        fs::write(base.join("questions/a.txt"), "Title A\n=====\n\nQ1\nWhy?\n\nA. x\nB. y\n\nAnswer: B\n").unwrap();
        fs::write(base.join("questions/bad.txt"), "no questions here").unwrap();
        fs::write(base.join("samples/s.json"), r#"{"title":"S","questions":[{"text":"q","choices":[{"text":"a","correct":true}]}]}"#).unwrap();

        let rows = scan(&base);
        let got: Vec<(&str, Result<(String, usize), String>)> = rows.iter().map(|r| (r.name.as_str(), r.info.clone())).collect();
        assert_eq!(got, [
            ("samples\\s.json", Ok(("S".into(), 1))),
            ("questions\\a.json", Ok(("Title A".into(), 1))),
            ("questions\\bad.txt", Err("cannot convert: no questions found".into())),
        ]);
        assert!(base.join("questions/a.txt").is_file(), "the txt stays");
        assert!(!base.join("questions/bad.json").exists());

        // a json that is already there is never overwritten
        fs::write(base.join("questions/a.json"), r#"{"title":"Edited","questions":[{"text":"q","choices":[{"text":"a","correct":true}]}]}"#).unwrap();
        assert_eq!(scan(&base)[1].info, Ok(("Edited".into(), 1)));
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
        let rows = [Entry { name: "samples\\demo.json".into(), path: PathBuf::new(), info: Ok(("Demo".into(), 3)) }];
        let mut buf = vec![];
        render_menu(&mut buf, &rows, [true, false], 2, (80, 40)).unwrap();
        let text = plain(buf);
        for want in ["exam-cli", " samples\\demo.json   Demo · 3 questions", "Options", " [x] always explain", "›[ ] don't show correct answers"] {
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
    fn screen_matches_the_mockup() {
        let exam = Exam { code: "CCIE-ENT".into(), questions: vec![q(QType::Single, &[false, true, false, false]); 100], ..Default::default() };
        let mut buf = vec![];
        render(&mut buf, &exam, 36, &st(&[true, false, false, false], true, false), false, [false; 2], true, (80, 40)).unwrap();
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
        render_review(&mut buf, &exam, &states, [true, true], 1, (80, 40)).unwrap();
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
        render_review(&mut buf, &exam, &states, [false; 2], 4, (80, 10)).unwrap();
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
