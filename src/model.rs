//! Exam data + the plain-text format VCE Designer imports (see its samples/import_sample.txt):
//! title + description, then "1. question", "A. choice"..., "Answer: A, B and F", explanation.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum QType {
    /// "fill" questions of older files have no choices left to show; they open as single select
    #[default]
    #[serde(alias = "fill")]
    Single,
    Multiple,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Choice {
    pub text: String,
    pub correct: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Question {
    #[serde(rename = "type")]
    pub kind: QType,
    pub text: String,
    /// data: URL, so one .json file carries everything
    pub image: String,
    pub choices: Vec<Choice>,
    pub explanation: String,
    pub shuffle: bool,
}

impl Default for Question {
    fn default() -> Self {
        Question { kind: QType::Single, text: String::new(), image: String::new(), choices: vec![], explanation: String::new(), shuffle: true }
    }
}

/// version 0.2 files held several exams ("Exam A", "Exam B"); read only to merge them
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct SubExam {
    pub name: String,
    pub questions: Vec<Question>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Exam {
    pub title: String,
    pub code: String,
    pub version: String,
    pub description: String,
    pub show_description: bool,
    /// out of 1000, as in VCE
    pub passing_score: u32,
    /// minutes, 0 = no limit
    pub time_limit: u32,
    pub questions: Vec<Question>,
    #[serde(skip_serializing)]
    pub exams: Vec<SubExam>,
}

impl Default for Exam {
    fn default() -> Self {
        Exam {
            title: "Untitled Exam".into(),
            code: "000-000".into(),
            version: "1.0".into(),
            description: String::new(),
            show_description: false,
            passing_score: 800,
            time_limit: 120,
            questions: vec![],
            exams: vec![],
        }
    }
}

impl Exam {
    /// a new file: one empty question to start typing into
    pub fn new_file() -> Self {
        Exam { questions: vec![Question::blank()], ..Default::default() }
    }

    /// merge the exams of older files into one question list
    pub fn fixup(&mut self) {
        for ex in std::mem::take(&mut self.exams) {
            self.questions.extend(ex.questions);
        }
    }
}

impl Question {
    pub fn blank() -> Self {
        Question { choices: (0..4).map(|i| Choice { text: String::new(), correct: i == 0 }).collect(), ..Default::default() }
    }

    pub fn is_valid(&self) -> bool {
        !self.text.trim().is_empty() && self.choices.iter().any(|c| c.correct) && self.choices.iter().all(|c| !c.text.trim().is_empty())
    }

    pub fn is_right(&self, picks: &[usize]) -> bool {
        self.choices.iter().enumerate().all(|(j, c)| c.correct == picks.contains(&j))
    }
}

pub fn letter(i: usize) -> char {
    (b'A' + i as u8) as char
}

fn join_lines(v: &[&str]) -> String {
    let mut s = v.join("\n");
    while s.contains("\n\n\n") {
        s = s.replace("\n\n\n", "\n\n");
    }
    s.trim().to_string()
}

/// "12. rest" -> (12, "rest")
fn q_start(line: &str) -> Option<(usize, &str)> {
    // "Q12" alone on a line, the question text follows on the next lines (Anthropic_CCDV-F.txt style)
    if let Some(num) = line.trim_end().strip_prefix('Q').filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
        return Some((num.parse().ok()?, ""));
    }
    let d = line.bytes().take_while(u8::is_ascii_digit).count();
    let rest = line.get(d..)?.strip_prefix('.')?;
    if d == 0 || !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some((line[..d].parse().ok()?, rest.trim_start()))
}

/// "B. rest" -> ('B', "rest")
fn choice_start(line: &str) -> Option<(char, &str)> {
    let mut cs = line.chars();
    let c = cs.next().filter(char::is_ascii_uppercase)?;
    let rest = cs.as_str().strip_prefix('.')?;
    rest.starts_with(char::is_whitespace).then(|| (c, rest.trim_start()))
}

#[derive(PartialEq)]
enum St {
    Text,
    Choices,
    Expl,
}

#[derive(Default)]
struct Draft<'a> {
    text: Vec<&'a str>,
    choices: Vec<Choice>,
    answer: String,
    expl: Vec<&'a str>,
}

impl Draft<'_> {
    fn finish(self) -> Question {
        let mut q = Question { text: join_lines(&self.text), explanation: join_lines(&self.expl), choices: self.choices, ..Default::default() };
        // single capital letters in "A, B and F"
        for tok in self.answer.split(|c: char| !c.is_alphanumeric()) {
            if tok.len() == 1 && tok.as_bytes()[0].is_ascii_uppercase() {
                if let Some(c) = q.choices.get_mut((tok.as_bytes()[0] - b'A') as usize) {
                    c.correct = true;
                }
            }
        }
        if q.choices.iter().filter(|c| c.correct).count() > 1 {
            q.kind = QType::Multiple;
        }
        q
    }
}

/// what a TXT file holds: one list of questions
#[derive(PartialEq, Debug, Default)]
pub struct TxtExam {
    pub title: String,
    pub description: String,
    pub questions: Vec<Question>,
}

pub fn parse_txt(src: &str) -> TxtExam {
    let src = src.trim_start_matches('\u{feff}').replace('\r', "");
    let mut exam = TxtExam::default();
    let mut head = vec![];
    let mut q: Option<Draft> = None;
    let mut st = St::Text;
    for line in src.split('\n') {
        if let Some((n, rest)) = q_start(line) {
            // only the next expected number starts a question, so numbered lists inside text stay text
            if (q.is_none() || st == St::Expl) && n == exam.questions.len() + if q.is_some() { 2 } else { 1 } {
                if let Some(d) = q.take() {
                    exam.questions.push(d.finish());
                }
                q = Some(Draft { text: vec![rest], ..Default::default() });
                st = St::Text;
                continue;
            }
        }
        let Some(d) = q.as_mut() else {
            // "=====" rules under the header are decoration
            if !(line.len() >= 3 && line.bytes().all(|b| b == b'=')) {
                head.push(line);
            }
            continue;
        };
        if st != St::Expl {
            if let Some(a) = line.get(..7).filter(|p| p.eq_ignore_ascii_case("answer:")) {
                d.answer = line[a.len()..].trim().to_string();
                st = St::Expl;
                continue;
            }
            if let Some((c, rest)) = choice_start(line) {
                if d.choices.len() < 26 && c == letter(d.choices.len()) {
                    d.choices.push(Choice { text: rest.to_string(), correct: false });
                    st = St::Choices;
                    continue;
                }
            }
        }
        match st {
            St::Choices => {
                if !line.trim().is_empty() {
                    let c = d.choices.last_mut().unwrap();
                    c.text.push('\n');
                    c.text.push_str(line);
                }
            }
            St::Expl => d.expl.push(line),
            St::Text => d.text.push(line),
        }
    }
    if let Some(d) = q {
        exam.questions.push(d.finish());
    }
    let h = join_lines(&head);
    let mut it = h.split('\n');
    exam.title = it.next().unwrap_or("").trim().to_string();
    exam.description = join_lines(&it.collect::<Vec<_>>());
    exam
}

pub fn to_txt(exam: &TxtExam) -> String {
    let mut out = vec![exam.title.clone(), String::new(), exam.description.clone(), String::new()];
    for (i, q) in exam.questions.iter().enumerate() {
        out.push(format!("{}. {}", i + 1, q.text));
        out.push(String::new());
        out.extend(q.choices.iter().enumerate().map(|(j, c)| format!("{}. {}", letter(j), c.text)));
        out.push(String::new());
        let right: Vec<String> = q.choices.iter().enumerate().filter(|(_, c)| c.correct).map(|(j, _)| letter(j).to_string()).collect();
        out.push(format!("Answer: {}", right.join(", ")));
        out.push(String::new());
        out.push(q.explanation.clone());
        out.push(String::new());
    }
    out.join("\r\n")
}

/// Windows-1252 bytes 0x80..=0x9F; the rest of 0xA0..=0xFF is Latin-1 as is.
const CP1252_HI: &str = "€\u{FFFD}‚ƒ„…†‡ˆ‰Š‹Œ\u{FFFD}Ž\u{FFFD}\u{FFFD}‘’“”•–—˜™š›œ\u{FFFD}žŸ";

/// UTF-8 if valid, otherwise Windows-1252 (what older Windows editors save English text as).
pub fn decode_text(b: &[u8]) -> String {
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b
            .iter()
            .map(|&c| match c {
                0x80..=0x9F => CP1252_HI.chars().nth((c - 0x80) as usize).unwrap(),
                _ => c as char,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INLINE: &str = "Demo\n\nDesc\n\n1. Pick one.\n\nA. x\nB. y\n\nAnswer: B\n\nBecause.\n\n\
        2. Pick two. Steps:\n1. first\n2. second\n\nA. a\nB. b\nC. c\n\nAnswer: A and C\n";

    fn correct(q: &Question) -> Vec<usize> {
        q.choices.iter().enumerate().filter(|(_, c)| c.correct).map(|(i, _)| i).collect()
    }

    #[test]
    fn inline_sample_and_round_trip() {
        let e = parse_txt(INLINE);
        assert_eq!(e.title, "Demo");
        assert_eq!(e.questions.len(), 2);
        assert!(e.questions[1].text.contains("2. second"));
        assert_eq!(correct(&e.questions[1]), [0, 2]);
        assert_eq!(e.questions[1].kind, QType::Multiple);
        assert!(e.questions[1].is_right(&[0, 2]));
        assert!(!e.questions[1].is_right(&[0]));
        assert_eq!(parse_txt(&to_txt(&e)), e);
    }

    #[test]
    fn q_line_format() {
        let src = "Anthropic CCDV-F\nTotal questions: 2\nDownloaded: 2026-09-15 23:05\n=====\n\n\
            Q1\nFirst line.\n\nSecond paragraph?\n\nA. one\nB. two\n\nAnswer: B\n\n\
            Q2\nPick two.\n\nA. a\nB. b\nC. c\n\nAnswer: A, C\n";
        let e = parse_txt(src);
        assert_eq!(e.title, "Anthropic CCDV-F");
        assert!(!e.description.contains('='));
        assert_eq!(e.questions.len(), 2);
        assert_eq!(e.questions[0].text, "First line.\n\nSecond paragraph?");
        assert_eq!((correct(&e.questions[0]), e.questions[0].kind), (vec![1], QType::Single));
        assert_eq!((correct(&e.questions[1]), e.questions[1].kind), (vec![0, 2], QType::Multiple));
    }

    #[test]
    fn ccdv_file() {
        let Ok(b) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/questions/Anthropic_CCDV-F.txt")) else { return };
        let e = parse_txt(&decode_text(&b));
        assert_eq!(e.questions.len(), 53);
        assert!(e.questions.iter().all(|q| q.choices.len() == 4 && correct(q).len() == 1 && !q.text.is_empty()));
        assert!(e.questions[52].text.starts_with("Your Claude application"));
    }

    #[test]
    fn vce_sample() {
        let Ok(b) = std::fs::read(r"C:\Program Files (x86)\VCE Exam Simulator Demo\samples\import_sample.txt") else { return };
        let e = parse_txt(&decode_text(&b));
        assert_eq!(e.title, "Standard Exam Demonstration");
        assert_eq!(e.questions.len(), 5);
        assert_eq!(correct(&e.questions[0]), [0]);
        assert!(e.questions[2].text.contains("<MyControls:MenuBar"));
        assert_eq!(correct(&e.questions[3]), [0, 1, 5]);
        assert!(e.questions[4].explanation.starts_with("The cascade"));
        assert_eq!(parse_txt(&to_txt(&e)), e);
    }

    #[test]
    fn old_file_migrates() {
        let json = r#"{"title":"T","exams":[{"name":"Exam A","questions":[{"type":"fill","text":"x","section":"S","answer":"1"}]},{"name":"Exam B","questions":[{"text":"y"}]}]}"#;
        let mut e: Exam = serde_json::from_str(json).unwrap();
        e.fixup();
        assert_eq!(e.questions.iter().map(|q| q.text.as_str()).collect::<Vec<_>>(), ["x", "y"]);
        assert_eq!(e.questions[0].kind, QType::Single);
        assert!(e.questions[1].shuffle);
        assert!(serde_json::to_value(&e).unwrap().get("exams").is_none());
    }

    #[test]
    fn cp1252() {
        assert_eq!(CP1252_HI.chars().count(), 32);
        assert_eq!(decode_text(b"caf\xE9 \x93hi\x94 \x80 company\x92s"), "café “hi” € company’s");
        assert_eq!(decode_text("\u{feff}naïve".as_bytes()), "naïve");
    }
}
