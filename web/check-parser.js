// node web/check-parser.js [file.txt ...]: runs the exam logic of exam.html under node and checks that it reads .txt
// exams exactly like exam-cli did (exam-cli writes name.json next to name.txt; that json is the reference).
// Without arguments it checks every .txt that has a .json next to it in samples/ and questions/.
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const html = fs.readFileSync(path.join(__dirname, 'exam.html'), 'utf8');
const logic = html.slice(html.indexOf('/* logic:start'), html.indexOf('/* logic:end */'));
const ctx = { TextDecoder };
vm.runInNewContext(logic + '\nthis.readExam = readExam; this.newState = newState; this.withAnswer = withAnswer; this.enter = enter; this.isRight = isRight;', ctx);

const root = path.join(__dirname, '..');
const args = process.argv.slice(2);
const txts = args.length ? args : ['samples', 'questions'].flatMap((d) => {
  const dir = path.join(root, d);
  return fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => /\.txt$/i.test(f)).map((f) => path.join(dir, f)) : [];
}).filter((f) => fs.existsSync(f.replace(/\.txt$/i, '.json')));

let bad = 0;
for (const txt of txts) {
  const got = ctx.readExam(path.basename(txt), fs.readFileSync(txt));
  const want = JSON.parse(fs.readFileSync(txt.replace(/\.txt$/i, '.json'), 'utf8'));
  const diff = [];
  if (got.title !== want.title) diff.push(`title ${JSON.stringify(got.title)} vs ${JSON.stringify(want.title)}`);
  if (got.questions.length !== want.questions.length) diff.push(`${got.questions.length} vs ${want.questions.length} questions`);
  got.questions.forEach((q, k) => {
    const w = want.questions[k];
    if (!w || diff.length > 5) return;
    const right = (x) => x.choices.map((c) => (c.correct ? 1 : 0)).join('');
    if (q.text !== w.text) diff.push(`Q${k + 1} text differs`);
    if (q.type !== w.type) diff.push(`Q${k + 1} type ${q.type} vs ${w.type}`);
    if (right(q) !== right(w)) diff.push(`Q${k + 1} answer ${right(q)} vs ${right(w)}`);
    if (q.choices.map((c) => c.text).join('|') !== w.choices.map((c) => c.text).join('|')) diff.push(`Q${k + 1} options differ`);
    if (q.explanation !== w.explanation) diff.push(`Q${k + 1} explanation differs`);
  });
  bad += diff.length ? 1 : 0;
  console.log(`${diff.length ? 'DIFF' : 'ok  '} ${path.relative(root, txt)} (${got.questions.length} questions)${diff.length ? '\n     ' + diff.join('\n     ') : ''}`);
}

// the answer rules, as in exam-cli's tests
const q = (type, correct) => ({ type, text: '?', choices: correct.map((c) => ({ text: 'x', correct: c })) });
const single = q('single', [false, false, true, false]);
const s = ctx.withAnswer(single);
const multi = q('multiple', [false, true, false, true]);
const m = ctx.newState(multi);
m.cursor = 1;
const checks = [
  [s.show && s.cursor === 2 && s.picks.join() === 'false,false,true,false', 'always show answers selects the right single answer'],
  [ctx.enter(multi, m) === false && m.picks[1] && !m.submitted, 'Enter on an unchosen multiple row chooses it'],
  [ctx.enter(multi, m) === false && m.submitted, 'Enter on a chosen row checks'],
  [ctx.enter(multi, m) === true, 'Enter after checking goes on'],
  [!ctx.isRight(multi, m), 'one of two right answers is not right'],
];
for (const [ok, what] of checks) { if (!ok) bad++; console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); }
process.exit(bad ? 1 : 0);
