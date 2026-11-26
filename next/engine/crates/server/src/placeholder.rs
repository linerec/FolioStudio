//! Type ▸ Fill with Placeholder Text. InDesign appends random Latin-looking text after the insertion point (or fills an
//! empty frame) and stops just before the story would run over its last frame (measured in InDesign 2025:
//! artifacts/placeholder-20261025). The words are Folio's own, made from syllables; only the rule "fill to the brim without
//! overset" is InDesign's.
use folio_edit::{Command, Editor, commands::RegisteredCommand};

const START: [&str; 18] = ["b", "c", "d", "f", "g", "l", "m", "n", "p", "qu", "r", "s", "t", "v", "ex", "in", "con", "per"];
const MID: [&str; 12] = ["a", "e", "i", "o", "u", "ae", "ia", "io", "al", "er", "is", "um"];
const END: [&str; 12] = ["m", "s", "t", "x", "re", "ra", "ro", "tur", "ntur", "bus", "ris", "nem"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize { (self.next() >> 33) as usize % n }
}

fn word(r: &mut Rng) -> String {
    let mut w = String::new();
    for i in 0..1 + r.below(3) {
        if i == 0 || r.below(3) > 0 { w.push_str(START[r.below(START.len())]); }
        w.push_str(MID[r.below(MID.len())]);
    }
    if r.below(3) > 0 { w.push_str(END[r.below(END.len())]); }
    w
}

/// The first `words` words of the text for `seed`: sentences end with . ? or !, commas fall inside them, and a paragraph
/// ends after about a hundred words. A longer text always starts with a shorter one.
pub fn text(seed: u64, words: usize) -> String {
    let mut r = Rng(seed | 1);
    let (mut out, mut sentence, mut paragraph, mut limit) = (String::new(), 0usize, 0usize, 8 + r.below(10));
    let mut para_limit = 70 + r.below(60);
    for _ in 0..words {
        let mut w = word(&mut r);
        if sentence == 0 { w = w[..1].to_uppercase() + &w[1..]; }
        out.push_str(&w);
        sentence += 1;
        paragraph += 1;
        if sentence >= limit {
            out.push_str([". ", ". ", ". ", "? ", "! "][r.below(5)]);
            sentence = 0;
            limit = 5 + r.below(12);
            if paragraph >= para_limit {
                out.pop();
                out.push('\r');
                paragraph = 0;
                para_limit = 70 + r.below(60);
            }
        } else if r.below(6) == 0 {
            out.push_str(", ");
        } else {
            out.push(' ');
        }
    }
    out.trim_end_matches([' ', ',']).to_owned()
}

fn seed(story: &str, at: usize) -> u64 {
    story.bytes().fold(0xcbf2_9ce4_8422_2325u64 ^ at as u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// The largest text that still leaves `story` without overset once inserted at `at`; `over` tells whether a trial
/// document (the parts with the text in) has the story overset.
pub fn fit(story: &str, at: usize, mut over: impl FnMut(&str) -> Result<bool, String>) -> Result<String, String> {
    let s = seed(story, at);
    if over("")? { return Ok(String::new()); }
    let (mut ok, mut bad) = (0usize, 64usize);
    while !over(&text(s, bad))? {
        ok = bad;
        bad *= 2;
        if bad > 100_000 { return Ok(text(s, ok)); }
    }
    while bad - ok > 1 {
        let mid = (ok + bad) / 2;
        if over(&text(s, mid))? { bad = mid } else { ok = mid }
    }
    Ok(text(s, ok))
}

/// Fill in the text of each `fillPlaceholderText` command from the current document.
pub fn prepare(commands: &mut [Command], parts: &std::collections::BTreeMap<String, String>, mut trial: impl FnMut(Editor, &str) -> Result<bool, String>) -> Result<(), String> {
    for c in commands.iter_mut() {
        let Command::Registered(RegisteredCommand::FillPlaceholderText { story, at, text }) = c else { continue };
        if !text.is_empty() { continue; }
        let (st, a) = (story.clone(), *at);
        *text = fit(&st, a, |t| {
            let mut ed = Editor::new(parts.clone());
            if !t.is_empty() {
                ed.apply("trial", &[Command::InsertText { story: st.clone(), at: a, text: t.to_owned() }]).map_err(|e| e.message)?;
            }
            trial(ed, &st)
        })?;
    }
    Ok(())
}
