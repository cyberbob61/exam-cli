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
use model::{Exam, QType, Question};
use std::io::{self, Write};

const TEAL: Color = Color::Rgb { r: 86, g: 196, b: 160 };
const BLUE: Color = Color::Rgb { r: 122, g: 170, b: 232 };
const TEXT: Color = Color::Rgb { r: 210, g: 208, b: 200 };
const WHITE: Color = Color::Rgb { r: 245, g: 245, b: 245 };
const DIM: Color = Color::Rgb { r: 130, g: 130, b: 130 };
const PURPLE: Color = Color::Rgb { r: 60, g: 52, b: 137 };
const GREEN: Color = Color::Rgb { r: 37, g: 80, b: 11 };
const RED: Color = Color::Rgb { r: 122, g: 32, b: 32 };
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
        self.cursor = if up { (self.cursor + n - 1) % n } else { (self.cursor + 1) % n };
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
fn row(q: &Question, s: &State, j: usize) -> Row {
    match (s.submitted, s.picks[j], q.choices[j].correct) {
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
}

impl<W: Write> Screen<'_, W> {
    /// one line; with a background it fills the whole row and puts `right` at the end
    fn line(&mut self, text: &str, fg: Color, bg: Option<Color>, right: &str) -> io::Result<()> {
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

    fn rule(&mut self) -> io::Result<()> {
        let n = RULE.min(self.width.saturating_sub(2 * MARGIN as usize));
        self.line(&"─".repeat(n), TEXT, None, "")
    }
}

fn exam_id(exam: &Exam) -> &str {
    if !exam.code.trim().is_empty() && exam.code != "000-000" { &exam.code } else { &exam.title }
}

// shortcut: lines past the bottom of the terminal are cut off; add scrolling if questions get that long
fn render<W: Write>(out: &mut W, exam: &Exam, i: usize, s: &State, help: bool, width: usize) -> io::Result<()> {
    let q = &exam.questions[i];
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width };
    sc.line(&format!("exam-cli · {} · TRAINING MODE", exam_id(exam)), TEAL, None, "")?;
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
        let label = format!("{mark}[{}] {}", model::letter(j), c.text.replace('\n', " "));
        match row(q, s, j) {
            Row::Plain => sc.line(&label, TEXT, None, "")?,
            Row::Selected => sc.line(&label, WHITE, Some(PURPLE), "")?,
            Row::Correct => sc.line(&label, WHITE, Some(GREEN), "CORRECT")?,
            Row::Incorrect => sc.line(&label, WHITE, Some(RED), "INCORRECT")?,
            Row::Answer => sc.line(&label, WHITE, Some(GREEN), "")?,
        }
    }
    if s.explain {
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
        queue!(sc.out, cursor::MoveTo(MARGIN, sc.y), SetForegroundColor(TEXT), Print(format!("Progress {bar} {}/{n}   ", i + 1)), SetForegroundColor(DIM), Print("R review   H help"), ResetColor)?;
    }
    out.flush()
}

/// list of all questions with their result, shown after the last question
fn render_review<W: Write>(out: &mut W, exam: &Exam, states: &[State], cur: usize, width: usize) -> io::Result<()> {
    let n = exam.questions.len();
    let inner = width.saturating_sub(2 * MARGIN as usize);
    queue!(out, terminal::Clear(ClearType::All))?;
    let mut sc = Screen { out, y: 1, width };
    sc.line(&format!("exam-cli · {} · TRAINING MODE", exam_id(exam)), TEAL, None, "")?;
    sc.line("review", BLUE, None, "")?;
    sc.rule()?;
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
        sc.line(&title, WHITE, Some(bg), label)?;
    }
    sc.rule()?;
    queue!(
        sc.out,
        cursor::MoveTo(MARGIN, sc.y),
        SetForegroundColor(TEXT),
        Print(format!("( {n} Questions , {wrong} Incorrect )   ")),
        SetForegroundColor(DIM),
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

/// puts the terminal back even if the trainer panics
struct Raw;
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
    }
}

fn run(exam: &Exam) -> io::Result<()> {
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
            Some(cur) => render_review(&mut out, exam, &states, cur, width as usize)?,
            None => render(&mut out, exam, i, &states[i], help, width as usize)?,
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
                KeyCode::Up => review = Some((cur + n - 1) % n),
                KeyCode::Down => review = Some((cur + 1) % n),
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
    // no file given: the demo next to the exe, else ask
    let demo = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("samples").join("demo.json"))).filter(|p| p.is_file());
    let path = match (arg, demo) {
        (Some(p), _) => std::path::PathBuf::from(p),
        (None, Some(d)) => d,
        (None, None) => match rfd::FileDialog::new().set_title("Open exam").add_filter("Exam (JSON)", &["json"]).pick_file() {
            Some(p) => p,
            None => return,
        },
    };
    let exam = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Exam>(&s).map_err(|e| e.to_string()));
    let mut exam = exam.unwrap_or_else(|e| fail(format!("cannot open {}: {e}", path.display())));
    exam.fixup();
    if exam.questions.is_empty() {
        fail(format!("{}: the exam has no questions", path.display()));
    }
    if let Err(e) = run(&exam) {
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
        assert_eq!(row(&one, &st(&[false, true, false, false], false, false), 1), Row::Selected);
        // submitted right: green with CORRECT
        assert_eq!(row(&one, &st(&[false, true, false, false], true, false), 1), Row::Correct);
        // submitted A (wrong): A red, B green without label
        let wrong = st(&[true, false, false, false], true, false);
        assert_eq!((row(&one, &wrong, 0), row(&one, &wrong, 1), row(&one, &wrong, 2)), (Row::Incorrect, Row::Answer, Row::Plain));
        // S before answering: right rows green, the rest untouched
        let multi = q(QType::Multiple, &[true, true, false, true]);
        let shown = st(&[false, false, true, false], false, true);
        assert_eq!((0..4).map(|j| row(&multi, &shown, j)).collect::<Vec<_>>(), [Row::Answer, Row::Answer, Row::Selected, Row::Answer]);
    }

    #[test]
    fn arrows_space_and_enter() {
        let one = q(QType::Single, &[false, true, false, false]);
        let mut s = State::new(&one);
        assert_eq!((s.cursor, s.picks.clone()), (0, vec![true, false, false, false])); // A chosen by default
        s.arrow(&one, false);
        assert_eq!(s.picks, [false, true, false, false]);
        s.arrow(&one, true);
        s.arrow(&one, true); // wraps from A to D
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
    fn keys_work_in_russian_layout() {
        let got: String = "ФИСВУАЫЧТЗРЙШНТ".chars().map(latin).collect();
        assert_eq!(got, "abcdefsxnphqiyn");
        assert_eq!((latin('B'), latin('q'), latin('7')), ('b', 'q', '7'));
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
        render(&mut buf, &exam, 36, &st(&[true, false, false, false], true, false), false, 80).unwrap();
        let text = plain(buf);
        for want in ["exam-cli · CCIE-ENT · TRAINING MODE", "single-choice", "Q37. Which?", " [A] x", "INCORRECT", "37/100", "H help"] {
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
        render_review(&mut buf, &exam, &states, 1, 80).unwrap();
        let text = plain(buf);
        for want in ["review", " Q1. Which?", "›Q2. Which?", "CORRECT", "INCORRECT", "UNANSWERED", "( 3 Questions , 2 Incorrect )"] {
            assert!(text.contains(want), "missing {want:?} in {text:?}");
        }
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
