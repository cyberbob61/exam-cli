# exam

Exam trainer in the style of a Pearson VUE exam, built for practice (training mode), in two versions:

- **Web page** for Windows, Linux and macOS: one file, `exam.html`, opened in a browser. Nothing to install.
- **Android app**, an APK.

Both read the same exam files: `.json`, and `.txt` in the formats below.

## Download

Each release has:

| File | For |
|------|-----|
| `exam.html` | Windows, Linux, macOS: open it in Chrome, Edge, Firefox or Safari |
| `exam-<ver>-android.apk` | Android 8.0+ |

They are built by `.github/workflows/build.yml` on every push to `master`; a `v*` tag publishes them as a release.

## Web page (Windows, Linux, macOS)

`web/exam.html` runs from disk: double-click it. It needs no install and no server, and works offline.

- **Open folder** (or drop a folder or exam files on the page) shows the folder as a list: the subfolders
  (and `..` inside them) and the exams with their titles and question counts, `.json` and `.txt` that has no
  `.json` beside it. ↑/↓ choose, Enter opens a folder or an exam, Backspace goes up a folder. A browser cannot
  remember a folder, so open it again each time you open the page.
- Options under the list (↑/↓ to them, Enter or Space to toggle); the page remembers them:
  - `always explain`: the explanation opens by itself once you check an answer (or once the answer is shown).
  - `don't show correct answers`: Enter only locks your answer; results (and the automatic
    explanation) appear in the review, at the end or with R.
  - `always show answers`: every question opens with the right options already green, as if S
    was pressed (S still hides them), and the `›` pointer on the first right option.

  The chosen options are shown in the header: `exam · DEMO-01 · always explain`.
- Skin, the row under the options (← / → or a click to change it; the page remembers it):
  - `terminal`: the console look, one screen of text with a bar of keys at the bottom.
  - `http_pro`: a practice-test page, the question as a card with Previous, Finish & score and Check answer,
    and beside it the accuracy count and an answer sheet of every question (click one to jump to it).
    A question opens with nothing chosen: the first Enter chooses the option under the pointer, the next one checks.
    Changing the question keeps the page scrolled where it was.
- A `.txt` exam is read on the fly; the page cannot write a `.json` beside it.
- Every key below also has a button in the bottom bar, and options can be clicked.

### Keys

Keys act at once. They also work with a Russian keyboard layout (the key at the same place counts, e.g. `ф` is `A`).

| Key | Action |
|-----|--------|
| A–D, F | choose option (E is Explain, so option E is chosen with the arrows or a click) |
| ↑ / ↓ | single-choice: move the selection; multiple-choice: move the `›` cursor |
| Space | choose / toggle the option under the cursor |
| Enter | single-choice: checks the answer, then goes on. Multiple-choice: chooses the row under the cursor; on a row already chosen it checks, then goes on |
| ← / →, P / N | previous / next question |
| S | show answer (correct rows turn green) |
| E | explain |
| I | enlarge the question's picture |
| R | review all questions |
| H | toggle help |
| Q, Esc | back to the exam list (asks Y / N) |

Single-choice questions start with A selected.
After the last question, the second Enter opens the review list: every question with
`CORRECT`, `INCORRECT` or `UNANSWERED` and a `( N Questions , M Incorrect )` total
(unanswered counts as incorrect). In review, ↑/↓ choose, Enter opens a question, and Enter
on an answered question brings you back to the review.

| State | Background | Right-hand label |
|-------|------------|------------------|
| Selected (before submit) | purple | — |
| Chosen and correct | green | `CORRECT` |
| Chosen and wrong | red | `INCORRECT` |
| Correct but not chosen (after a wrong answer, or on `S`) | green | — |

`node web/check-parser.js` checks the page's exam reading and answer rules under node: every `.txt` in
`samples/` and `questions/` that has a `.json` beside it must read the same as that `.json`.

## Android

`android/` holds the phone app (Kotlin + Jetpack Compose), styled after the VCE mobile player:
a list of exam cards (title, file, size, ▶), and an exam screen with the exam name and a
progress bar on top, **Check** (marks the right answer green and a wrong pick red,
press again to hide) and ★ flag. Swipe left or right to move between questions, past the last
one to open the review, and sideways in the review to go back.
**Open file** brings in a `.json` or `.txt` (e.g. from Downloads), and a `.txt` gets a `.json` made next to it.
Long-press an exam card to delete it from the app (its `.json` and the `.txt` it came from; the file you opened
stays where it was). The list screen has the "always explain" option (the explanation opens with Check) and the
"always show answers" option (every question opens already checked: right answers green, and with "always explain"
the explanation below them). Install the APK from the release (allow installing from your browser or file manager
when Android asks).

Updates: every push to `master` replaces the `android-latest` pre-release with a new APK ("Android build N",
N is the CI run number). On start the app compares N with its own build number (shown under the exam list) and, if
GitHub has a newer one, offers to download and install it. All CI builds are signed with one permanent key
(repo secrets `ANDROID_KEYSTORE`, base64 PKCS12 with alias `exam`, and `ANDROID_KEYSTORE_PASSWORD`), so an update
installs over the installed app and keeps its exams. A local build without these secrets uses the Gradle debug key.
The app takes its demo exam from `samples/demo.json`.

## Exam file

JSON, one exam per file:

```json
{
  "title": "Demo Exam",
  "code": "DEMO-01",
  "questions": [
    {
      "type": "single",
      "text": "1 + 1 = ?",
      "choices": [
        { "text": "1", "correct": false },
        { "text": "2", "correct": true }
      ],
      "explanation": "One plus one equals two.",
      "image": ""
    }
  ]
}
```

- `type`: `single` or `multiple`.
- `image`: optional `data:image/png;base64,...` URL.
- Text between two lines of ```` ``` ```` in a question is shown as code.

Two text formats are read as well. A question without two options or without an answer (e.g. HOTSPOT) is left out.

```
Exam title                         Exam title
=====                              
                                   1. Question text
Q1                                 A. option
Question text                      B. option
                                   Answer: B
A. option                          
B. option                          Explanation text
                                   
Answer: B          (A, C = multiple choice)
```

## Design

The original spec and screenshots of the first, console version are in [docs/DESIGN.md](docs/DESIGN.md).
The web page keeps its screens and keys; exam files are JSON (not YAML), Explain is `E` (not `X`),
plus arrows, review and the quit question.
