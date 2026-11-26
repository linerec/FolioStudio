//! The GREP matcher of paragraph GREP styles: a backtracking matcher for the dialect InDesign 2025 (20.5.5) accepts.
//! Every rule below was measured with `oracle/indesign/grep-syntax-probe.jsx` (table: `oracle/rules/grep-syntax-cases.json`,
//! answers: `oracle/goldens/grep-syntax/indesign.json`, replayed by `grep::tests::indesign_table`). Where InDesign
//! disagrees with ICU, InDesign wins:
//! - an expression InDesign cannot parse styles nothing; Folio rejects it with the reason (`Err`).
//! - `^` and `$` also match at a forced line break (`\n`); `.` stops at `\n \r U+0085 U+2028 U+2029`; `\Z` is `\z`.
//! - a look-behind needs a fixed length (all alternatives equally long); `{` that is not a valid interval is a literal.
//! - `\<` and `\>` are word edges; `\p{...}` takes two-letter categories and a few names; inside `[...]` it is literal.
//! - `~` plus a code letter is a special character (`~_` em dash ...); any other `~` is a literal tilde.
//! - the scan stops at the first empty match (`a*` on `baa` styles nothing).
use folio_model::{marker, ParagraphStart};
use regex::Regex as Unicode;
use std::{collections::HashMap, ops::Range, sync::{Arc, Mutex, OnceLock}};

#[derive(Clone, Copy, PartialEq)]
enum Kind { Greedy, Lazy, Possessive }

#[derive(Clone, Copy, PartialEq, Debug)]
enum Pred { Digit, Word, Space, Lower, Upper, HSpace, VSpace, Posix(&'static str) }

#[derive(Clone)]
enum Item { Range(char, char), Pred(Pred, bool), Prop(Arc<Unicode>, bool) }

#[derive(Clone)]
struct Set { negated: bool, items: Vec<Item> }

enum Node {
    Empty,
    Char(char),
    Any,
    Set(Set),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { node: Box<Node>, min: u32, max: u32, kind: Kind },
    Group(Box<Node>, Option<usize>),
    Atomic(Box<Node>),
    Look { node: Box<Node>, ahead: bool, negative: bool, length: usize },
    BackRef(usize),
    /// `~M ~R ~P ~L ~E`: the return that ends a paragraph is this break character (matched only at the end of the subject).
    Break(ParagraphStart),
    Bol, Eol, WordEdge(bool), WordStart, WordEnd, Start, End, Contiguous,
    /// Case and dot-all flags apply to what is inside.
    Flags(Box<Node>, bool, bool),
}

pub struct Regex { root: Node, groups: usize, names_end_nested: bool, names_discretionary_break: bool, names_index: bool, names_indent_tab: bool }

/// The indent-to-here tab (`<?ACE 7?>`, U+0007 in `Story.contents`) as it stands in the model's text.
pub const INDENT_TAB: char = '\u{7}';

/// Whether a set takes the indent-to-here tab: a positive `\s` (or `[[:space:]]`) or a range that holds it. A negated set, `\S`, `\W`, `\H`
/// and `\h` do not (measured, rules/specialchars-c1.ko.md).
fn set_names_tab(s: &Set) -> bool {
    !s.negated && s.items.iter().any(|i| match i {
        Item::Range(a, b) => *a <= INDENT_TAB && INDENT_TAB <= *b,
        Item::Pred(Pred::Space, false) | Item::Pred(Pred::Posix("space"), false) => true,
        _ => false,
    })
}

/// Whether a part of the expression can match the indent-to-here tab.
fn names_tab(n: &Node) -> bool {
    match n {
        Node::Char(c) => *c == INDENT_TAB,
        Node::Set(s) => set_names_tab(s),
        Node::Seq(v) | Node::Alt(v) => v.iter().any(names_tab),
        Node::Repeat { node, .. } | Node::Group(node, _) | Node::Atomic(node) | Node::Look { node, .. } | Node::Flags(node, _, _) => names_tab(node),
        _ => false,
    }
}

fn props() -> &'static Mutex<HashMap<String, Arc<Unicode>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Unicode>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A one-character Unicode class such as `\p{Lu}` or `[\p{Alphabetic}\p{M}]`, built once.
fn unicode(class: &str) -> Result<Arc<Unicode>, String> {
    let mut cache = props().lock().expect("Unicode property cache lock is not poisoned");
    if let Some(r) = cache.get(class) { return Ok(r.clone()); }
    let r = Arc::new(Unicode::new(&format!(r"\A(?:{class})\z")).map_err(|e| format!("unsupported GREP property {class}: {e}"))?);
    cache.insert(class.to_owned(), r.clone());
    Ok(r)
}

fn is_class(re: &Unicode, c: char) -> bool { re.is_match(c.encode_utf8(&mut [0; 4])) }

fn pred(p: Pred, c: char) -> bool {
    let re = |s: &str| unicode(s).map(|r| is_class(&r, c)).unwrap_or(false);
    match p {
        Pred::Digit => c.is_ascii_digit() || (!c.is_ascii() && re(r"\p{Nd}")),
        Pred::Word => c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && re(r"\w")),
        // The indent-to-here and right-indent tab markers count as white space (measured: `\s` takes both).
        Pred::Space => c.is_whitespace() || matches!(c, '\u{85}' | '\u{7}' | '\u{8}'),
        Pred::Lower => c.is_ascii_lowercase() || (!c.is_ascii() && re(r"\p{Ll}")),
        Pred::Upper => c.is_ascii_uppercase() || (!c.is_ascii() && re(r"\p{Lu}")),
        Pred::HSpace => c == '\t' || c == ' ' || (!c.is_ascii() && re(r"\p{Zs}")),
        Pred::VSpace => matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'),
        Pred::Posix(name) => match name {
            "alpha" => c.is_ascii_alphabetic() || (!c.is_ascii() && re(r"\p{Alphabetic}")),
            "alnum" => c.is_ascii_alphanumeric() || (!c.is_ascii() && re(r"[\p{Alphabetic}\p{Nd}]")),
            "digit" => pred(Pred::Digit, c),
            "upper" => pred(Pred::Upper, c),
            "lower" => pred(Pred::Lower, c),
            "punct" => c.is_ascii_punctuation() && !"$+<=>^`|~".contains(c) || (!c.is_ascii() && re(r"\p{P}")),
            "space" => pred(Pred::Space, c),
            "blank" => pred(Pred::HSpace, c),
            "xdigit" => c.is_ascii_hexdigit(),
            "word" => pred(Pred::Word, c),
            "print" => !c.is_control() || c == '\r',
            "graph" => !c.is_whitespace() && !c.is_control(),
            "cntrl" => c.is_control(),
            _ => false,
        },
    }
}

fn simple_upper(c: char) -> char { let mut u = c.to_uppercase(); match (u.next(), u.next()) { (Some(x), None) => x, _ => c } }
fn simple_lower(c: char) -> char { let mut u = c.to_lowercase(); match (u.next(), u.next()) { (Some(x), None) => x, _ => c } }
fn variants(c: char) -> [char; 3] { [c, simple_lower(c), simple_upper(c)] }

impl Set {
    fn contains(&self, c: char, ci: bool) -> bool {
        let one = |c: char| self.items.iter().any(|i| match i {
            Item::Range(a, b) => *a <= c && c <= *b,
            Item::Pred(p, neg) => pred(*p, c) != *neg && !(*neg && c == '\u{ad}'),
            Item::Prop(re, neg) => is_class(re, c) != *neg && !(*neg && c == '\u{ad}'),
        });
        let hit = if ci { variants(c).into_iter().any(one) } else { one(c) };
        // A discretionary hyphen is never taken by a negated set (measured), only by an explicit match.
        if c == '\u{ad}' && self.negated { return false; }
        hit != self.negated
    }
}

// ---------- parsing

const CODES: &[(char, &str)] = &[
    ('_', "\u{2014}"), ('=', "\u{2013}"), ('-', "\u{ad}"), ('~', "\u{2011}"), ('{', "\u{201c}"), ('}', "\u{201d}\""), ('[', "\u{2018}"), (']', "\u{2019}'"),
    ('m', "\u{2003}"), ('>', "\u{2002}"), ('S', "\u{a0}"), ('s', "\u{202f}"), ('|', "\u{200a}"), ('<', "\u{2009}"), ('/', "\u{2007}"), ('.', "\u{2008}"),
    ('3', "\u{2004}"), ('%', "\u{2006}"), ('4', "\u{2005}"), ('e', "\u{2026}"), ('k', "\u{200b}"), ('j', "\u{200c}"), ('8', "\u{2022}"), ('2', "\u{a9}"),
    ('6', "\u{a7}"), ('7', "\u{b6}"), ('r', "\u{ae}"), ('d', "\u{2122}"), ('(', "\u{3000}"), ('i', "\u{7}"), ('y', "\u{8}"), ('f', "\u{2001}"),
    // Markers (measured: rules/specialchars-c1.ko.md). `~#` is any page number (current, next, previous), a text variable is none of them.
    // The characters are `folio_model::marker`'s (a test below keeps the two in step).
    ('N', "\u{10FF00}"), ('X', "\u{10FF01}"), ('V', "\u{10FF02}"), ('#', "\u{10FF00}\u{10FF01}\u{10FF02}"), ('x', "\u{10FF04}"),
    ('F', "\u{10FF05}"), ('h', "\u{10FF06}"), ('a', "\u{fffc}"),
];
/// Codes that stand for a break or marker which cannot be a member of a set (a break is the paragraph's own return: `~M` alone).
const MARKER_CODES: &str = "MRPLEI";
/// `~M ~R ~P ~L ~E`: the break character that ends a paragraph.
fn break_of(code: char) -> Option<ParagraphStart> {
    Some(match code {
        'M' => ParagraphStart::NextColumn,
        'R' => ParagraphStart::NextFrame,
        'P' => ParagraphStart::NextPage,
        'L' => ParagraphStart::NextOddPage,
        'E' => ParagraphStart::NextEvenPage,
        _ => return None,
    })
}

struct Parser { p: Vec<char>, at: usize, groups: usize, names: Vec<(String, usize)>, flags_used: bool, end_nested: bool, discretionary_break: bool, index: bool }

#[derive(Clone, Copy)]
struct Flags { ci: bool, dotall: bool, ext: bool }

impl Parser {
    fn peek(&self) -> Option<char> { self.p.get(self.at).copied() }
    fn peek_at(&self, n: usize) -> Option<char> { self.p.get(self.at + n).copied() }
    fn eat(&mut self, c: char) -> bool { if self.peek() == Some(c) { self.at += 1; true } else { false } }
    fn starts(&self, s: &str) -> bool { s.chars().enumerate().all(|(i, c)| self.peek_at(i) == Some(c)) }
    fn err<T>(&self, what: &str) -> Result<T, String> { Err(format!("unsupported GREP syntax at position {}: {what}", self.at)) }

    fn alt(&mut self, f: &mut Flags) -> Result<Node, String> {
        let mut alts = vec![self.seq(f)?];
        while self.eat('|') { alts.push(self.seq(f)?); }
        Ok(if alts.len() == 1 { alts.pop().expect("single alternative was checked above") } else { Node::Alt(alts) })
    }

    fn seq(&mut self, f: &mut Flags) -> Result<Node, String> {
        let mut items = Vec::new();
        loop {
            if f.ext { self.skip_free_space(); }
            match self.peek() {
                None | Some('|') | Some(')') => break,
                _ => {}
            }
            if self.starts("(?") && self.flags_only(f)? { continue; }
            let before = *f;
            let atom = self.atom(f)?;
            let atom = match atom { Some(a) => a, None => continue };
            let atom = self.quantified(atom, f)?;
            items.push(if before.ci || before.dotall || self.flags_used { Node::Flags(Box::new(atom), before.ci, before.dotall) } else { atom });
        }
        Ok(if items.len() == 1 { items.pop().expect("single sequence item was checked above") } else { Node::Seq(items) })
    }

    fn skip_free_space(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.at += 1,
                Some('#') => while let Some(c) = self.peek() { self.at += 1; if c == '\n' { break; } },
                _ => break,
            }
        }
    }

    /// `(?i)` `(?-i)` `(?s)` `(?x)` `(?m)` and combinations: they change the flags to the end of the group.
    fn flags_only(&mut self, f: &mut Flags) -> Result<bool, String> {
        let save = self.at;
        self.at += 2;
        let mut on = true;
        let mut next = *f;
        let mut any = false;
        while let Some(c) = self.peek() {
            match c {
                '-' => on = false,
                'i' => next.ci = on, 's' => next.dotall = on, 'x' => next.ext = on, 'm' => {},
                ')' => { self.at += 1; if any { *f = next; self.flags_used = true; return Ok(true); } else { self.at = save; return Ok(false); } }
                _ => { self.at = save; return Ok(false); }
            }
            any = true;
            self.at += 1;
        }
        self.at = save;
        Ok(false)
    }

    fn quantified(&mut self, atom: Node, f: &Flags) -> Result<Node, String> {
        let (min, max) = match self.peek() {
            Some('*') => { self.at += 1; (0, u32::MAX) }
            Some('+') => { self.at += 1; (1, u32::MAX) }
            Some('?') => { self.at += 1; (0, 1) }
            Some('{') => match self.interval() { Some(r) => r?, None => return Ok(atom) },
            _ => return Ok(atom),
        };
        let kind = if self.eat('?') { Kind::Lazy } else if self.eat('+') { Kind::Possessive } else { Kind::Greedy };
        if f.ext { self.skip_free_space(); }
        if matches!(self.peek(), Some('*') | Some('+') | Some('?')) { return self.err("repeated quantifier"); }
        Ok(Node::Repeat { node: Box::new(atom), min, max, kind })
    }

    /// `{n}` `{n,}` `{n,m}`; anything else after `{` is a literal brace (the caller sees `None`).
    fn interval(&mut self) -> Option<Result<(u32, u32), String>> {
        let rest: String = self.p[self.at + 1..].iter().take_while(|c| **c != '}').collect();
        if self.p.get(self.at + 1 + rest.chars().count()) != Some(&'}') { return None; }
        let (a, b) = match rest.split_once(',') { Some((a, b)) => (a, Some(b)), None => (rest.as_str(), None) };
        if a.is_empty() || !a.chars().all(|c| c.is_ascii_digit()) || b.is_some_and(|b| !b.chars().all(|c| c.is_ascii_digit())) { return None; }
        self.at += rest.chars().count() + 2;
        let min: u32 = match a.parse() { Ok(v) => v, Err(_) => return Some(self.err("interval too large")) };
        let max: u32 = match b { None => min, Some("") => u32::MAX, Some(b) => match b.parse() { Ok(v) => v, Err(_) => return Some(self.err("interval too large")) } };
        if min > max || min > 1000 && max != u32::MAX || max != u32::MAX && max > 1000 { return Some(self.err("bad interval")); }
        Some(Ok((min, max)))
    }

    fn atom(&mut self, f: &mut Flags) -> Result<Option<Node>, String> {
        let c = self.peek().expect("seq checks that an atom character remains");
        self.at += 1;
        Ok(Some(match c {
            '(' => return self.group(f).map(Some),
            '[' => Node::Set(self.set(f)?),
            '.' => Node::Any,
            '^' => Node::Bol,
            '$' => Node::Eol,
            '*' | '+' | '?' => return self.err("nothing to repeat"),
            '\\' => return self.escape(),
            '~' => match self.peek() {
                Some(code) if CODES.iter().any(|(k, _)| *k == code) => { self.at += 1; if code == 'h' { self.end_nested = true; }
                    if code == 'k' { self.discretionary_break = true; } code_node(code) }
                Some(code) if break_of(code).is_some() => { self.at += 1; Node::Break(break_of(code).expect("match guard checked this break code")) }
                // The index marker: an invisible character that only `~I` finds (measured, rules/specialchars-c1.ko.md).
                Some('I') => { self.at += 1; self.index = true; Node::Char(marker::INDEX) }
                Some('*' | '+' | '?' | '{') => return self.err("a quantifier right after ~ (measured: nothing matches)"),
                _ => Node::Char('~'),
            },
            c => Node::Char(c),
        }))
    }

    fn group(&mut self, f: &mut Flags) -> Result<Node, String> {
        let mut inner = *f;
        let mut capture = true;
        let mut wrap: Option<Box<dyn Fn(Node) -> Node>> = None;
        if self.eat('?') {
            capture = false;
            if self.eat(':') {
            } else if self.eat('>') { wrap = Some(Box::new(|n| Node::Atomic(Box::new(n)))); }
            else if self.eat('=') { wrap = Some(Box::new(|n| Node::Look { node: Box::new(n), ahead: true, negative: false, length: 0 })); }
            else if self.eat('!') { wrap = Some(Box::new(|n| Node::Look { node: Box::new(n), ahead: true, negative: true, length: 0 })); }
            else if self.starts("<=") || self.starts("<!") {
                let negative = self.peek_at(1) == Some('!');
                self.at += 2;
                let body = self.alt_in(&mut inner)?;
                let length = fixed_length(&body).ok_or_else(|| "unsupported GREP syntax: a look-behind needs a fixed length".to_string())?;
                return Ok(Node::Look { node: Box::new(body), ahead: false, negative, length });
            } else if self.eat('<') {
                let name: String = self.p[self.at..].iter().take_while(|c| **c != '>').collect();
                self.at += name.chars().count();
                if !self.eat('>') || name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) { return self.err("bad group name"); }
                self.groups += 1;
                let idx = self.groups;
                self.names.push((name, idx));
                let body = self.alt_in(&mut inner)?;
                return Ok(Node::Group(Box::new(body), Some(idx)));
            } else if self.eat('#') {
                while let Some(c) = self.peek() { self.at += 1; if c == ')' { return Ok(Node::Empty); } }
                return self.err("unterminated comment");
            } else {
                // `(?i:...)` and friends
                let mut on = true;
                loop {
                    match self.peek() {
                        Some('-') => on = false,
                        Some('i') => { inner.ci = on; self.flags_used = true; } Some('s') => { inner.dotall = on; self.flags_used = true; } Some('x') => inner.ext = on, Some('m') => {},
                        Some(':') => { self.at += 1; break; }
                        _ => return self.err("unknown group flag"),
                    }
                    self.at += 1;
                }
            }
        }
        let idx = if capture { self.groups += 1; Some(self.groups) } else { None };
        let body = self.alt_in(&mut inner)?;
        Ok(match wrap { Some(w) => w(body), None => Node::Group(Box::new(body), idx) })
    }

    fn alt_in(&mut self, f: &mut Flags) -> Result<Node, String> {
        let body = self.alt(f)?;
        if !self.eat(')') { return self.err("missing )"); }
        Ok(body)
    }

    fn escape(&mut self) -> Result<Option<Node>, String> {
        let Some(c) = self.peek() else { return self.err("dangling backslash") };
        self.at += 1;
        let shorthand = |p: Pred, neg: bool| Node::Set(Set { negated: neg, items: vec![Item::Pred(p, false)] });
        Ok(Some(match c {
            'd' => shorthand(Pred::Digit, false), 'D' => shorthand(Pred::Digit, true),
            'w' => shorthand(Pred::Word, false), 'W' => shorthand(Pred::Word, true),
            's' => shorthand(Pred::Space, false), 'S' => shorthand(Pred::Space, true),
            'l' => shorthand(Pred::Lower, false), 'L' => shorthand(Pred::Lower, true),
            'u' => shorthand(Pred::Upper, false), 'U' => shorthand(Pred::Upper, true),
            'h' => shorthand(Pred::HSpace, false), 'H' => shorthand(Pred::HSpace, true),
            'v' | 'R' => shorthand(Pred::VSpace, false),
            // A grapheme cluster: Folio takes one character (combining marks are not joined to their base).
            'X' => Node::Set(Set { negated: true, items: Vec::new() }),
            'b' => Node::WordEdge(true), 'B' => Node::WordEdge(false),
            'A' => Node::Start, 'Z' | 'z' => Node::End, 'G' => Node::Contiguous,
            '<' => Node::WordStart, '>' => Node::WordEnd,
            't' => Node::Char('\t'), 'n' => Node::Char('\n'), 'r' => Node::Char('\r'),
            'x' => { let c = self.hex()?; self.marker_hex(c) }
            // `\N{LATIN SMALL LETTER A}`: the character Unicode names so (measured: any letter case; `\N{U+0041}` is nothing).
            'N' => {
                if !self.eat('{') { return self.err("\\N needs {name}"); }
                let name: String = self.p[self.at..].iter().take_while(|c| **c != '}').collect();
                self.at += name.chars().count();
                if !self.eat('}') { return self.err("unterminated \\N{...}"); }
                match character_named(&name) { Some(c) => Node::Char(c), None => return self.err(&format!("\\N{{{name}}} is not a Unicode character name")) }
            }
            'Q' => {
                let mut lit = Vec::new();
                while self.at < self.p.len() && !self.starts("\\E") { lit.push(Node::Char(self.p[self.at])); self.at += 1; }
                if self.starts("\\E") { self.at += 2; }
                Node::Seq(lit)
            }
            'p' | 'P' => Node::Set(Set { negated: false, items: vec![self.property(c == 'P')?] }),
            '1'..='9' => {
                let mut n = c as usize - '0' as usize;
                while let Some(d) = self.peek().and_then(|d| d.to_digit(10)) {
                    if n * 10 + d as usize <= self.groups { n = n * 10 + d as usize; self.at += 1; } else { break; }
                }
                if n > self.groups { return self.err("back reference to a group that is not there"); }
                Node::BackRef(n)
            }
            'k' => {
                if !self.eat('<') { return self.err("bad named back reference"); }
                let name: String = self.p[self.at..].iter().take_while(|c| **c != '>').collect();
                self.at += name.chars().count();
                if !self.eat('>') { return self.err("bad named back reference"); }
                match self.names.iter().find(|(n, _)| *n == name) { Some((_, i)) => Node::BackRef(*i), None => return self.err("unknown group name") }
            }
            c if !c.is_alphanumeric() => Node::Char(c),
            c => return self.err(&format!("\\{c}")),
        }))
    }

    /// `\x{0018}` and friends name the character InDesign stores for a marker: here its stand-ins (`\x{0018}` is every page number
    /// and a text variable instance, measured).
    fn marker_hex(&mut self, c: char) -> Node {
        let stands = marker_of_code(c);
        if c == '\u{3}' { self.end_nested = true; }
        if c == '\u{200b}' { self.discretionary_break = true; }
        match stands.len() {
            0 => Node::Char(c),
            1 => Node::Char(stands[0]),
            _ => Node::Set(Set { negated: false, items: stands.iter().map(|m| Item::Range(*m, *m)).collect() }),
        }
    }

    fn hex(&mut self) -> Result<char, String> {
        let digits: String = if self.eat('{') {
            let d: String = self.p[self.at..].iter().take_while(|c| **c != '}').collect();
            self.at += d.chars().count();
            if !self.eat('}') { return self.err("bad \\x{...}"); }
            d
        } else {
            let d: String = self.p[self.at..].iter().take(2).take_while(|c| c.is_ascii_hexdigit()).collect();
            self.at += d.chars().count();
            d
        };
        u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32).ok_or_else(|| format!("unsupported GREP syntax at position {}: bad \\x escape", self.at))
    }

    /// `\p{Lu}`: two-letter categories and the long names InDesign knows; the one-letter classes behave oddly there
    /// (measured: `\p{L}` is `\p{Ll}`, `\p{N} \p{P} \p{M} \p{Z} \p{C}` match nothing), scripts and `\p{Alphabetic}` match nothing.
    fn property(&mut self, negated: bool) -> Result<Item, String> {
        let name: String = if self.eat('{') {
            let n: String = self.p[self.at..].iter().take_while(|c| **c != '}').collect();
            self.at += n.chars().count();
            if !self.eat('}') { return self.err("unterminated property name"); }
            n
        } else {
            // One letter without braces: the name is just that letter.
            let Some(c) = self.peek() else { return self.err("a property needs a name") };
            self.at += 1;
            c.to_string()
        };
        let key = name.to_ascii_lowercase();
        let class = match key.as_str() {
            "lu" | "uppercase_letter" => r"\p{Lu}", "ll" | "lowercase_letter" => r"\p{Ll}", "l" => r"\p{Ll}",
            "lt" | "titlecase_letter" => r"\p{Lt}", "lm" | "modifier_letter" => r"\p{Lm}", "lo" | "other_letter" => r"\p{Lo}", "letter" => r"\p{L}",
            "mn" | "nonspacing_mark" => r"\p{Mn}", "mc" => r"\p{Mc}", "me" => r"\p{Me}",
            "nd" | "decimal_number" => r"\p{Nd}", "nl" | "letter_number" => r"\p{Nl}", "no" | "other_number" => r"\p{No}",
            "pc" => r"\p{Pc}", "pd" => r"\p{Pd}", "ps" => r"\p{Ps}", "pe" => r"\p{Pe}", "pi" => r"\p{Pi}", "pf" => r"\p{Pf}", "po" => r"\p{Po}",
            "sm" => r"\p{Sm}", "sc" => r"\p{Sc}", "sk" => r"\p{Sk}", "so" => r"\p{So}",
            "zs" => r"\p{Zs}", "zl" => r"\p{Zl}", "zp" => r"\p{Zp}", "s" => r"[\p{Zs}\p{Zl}\p{Zp}]",
            "alpha" => r"\p{L}", "ascii" => r"[\x00-\x7f]",
            "n" | "p" | "m" | "z" | "c" | "cc" | "cf" | "lc" | "l&" => r"[^\x00-\u{10ffff}]",
            _ => return self.err(&format!("\\p{{{name}}} is not a property InDesign matches")),
        };
        Ok(Item::Prop(unicode(class)?, negated))
    }

    fn set(&mut self, f: &Flags) -> Result<Set, String> {
        let _ = f;
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else { return self.err("missing ]") };
            self.at += 1;
            if c == ']' && !first { break; }
            first = false;
            let lo = match c {
                '[' if self.peek() == Some(':') => {
                    let rest: String = self.p[self.at + 1..].iter().take_while(|c| **c != ':').collect();
                    let (neg, name) = match rest.strip_prefix('^') { Some(n) => (true, n), None => (false, rest.as_str()) };
                    let known = ["alpha", "alnum", "digit", "upper", "lower", "punct", "space", "blank", "xdigit", "word", "print", "graph", "cntrl"];
                    let Some(k) = known.iter().copied().find(|k| *k == name) else { return self.err("unknown POSIX class") };
                    self.at += rest.chars().count() + 2;
                    if !self.eat(']') { return self.err("bad POSIX class"); }
                    items.push(Item::Pred(Pred::Posix(k), neg));
                    continue;
                }
                '[' => return self.err("nested sets are not accepted"),
                '\\' => {
                    let Some(e) = self.peek() else { return self.err("dangling backslash") };
                    self.at += 1;
                    let p = |p: Pred, neg: bool| Some(Item::Pred(p, neg));
                    let shorthand = match e {
                        'd' => p(Pred::Digit, false), 'D' => p(Pred::Digit, true), 'w' => p(Pred::Word, false), 'W' => p(Pred::Word, true),
                        's' => p(Pred::Space, false), 'S' => p(Pred::Space, true), 'l' => p(Pred::Lower, false), 'L' => p(Pred::Lower, true),
                        'u' => p(Pred::Upper, false), 'U' => p(Pred::Upper, true), 'h' => p(Pred::HSpace, false), 'H' => p(Pred::HSpace, true),
                        'v' | 'R' => p(Pred::VSpace, false),
                        _ => None,
                    };
                    if let Some(s) = shorthand { items.push(s); continue; }
                    match e {
                        't' => '\t',
                        'n' => '\n',
                        'r' => '\r',
                        'x' => {
                            let c = self.hex()?;
                            let stands = marker_of_code(c);
                            if c == '\u{200b}' { self.discretionary_break = true; }
                            if !stands.is_empty() {
                                if c == '\u{3}' { self.end_nested = true; }
                                for m in stands { items.push(Item::Range(*m, *m)); }
                                continue;
                            }
                            c
                        }
                        e => e,
                    }
                }
                '~' => match self.peek() {
                    Some(code) if CODES.iter().any(|(k, _)| *k == code) => {
                        self.at += 1;
                        for ch in CODES.iter().find(|(k, _)| *k == code).expect("caller checked membership in CODES").1.chars() { items.push(Item::Range(ch, ch)); }
                        continue;
                    }
                    Some(code) if MARKER_CODES.contains(code) => return self.err(&format!("special code ~{code} cannot be a member of a set")),
                    _ => '~',
                },
                c => c,
            };
            if self.peek() == Some('-') && self.peek_at(1).is_some_and(|n| n != ']') {
                self.at += 1;
                let mut hi = self.peek().expect("range guard checked the character after the hyphen");
                self.at += 1;
                if hi == '\\' {
                    let e = self.peek().ok_or("dangling backslash")?;
                    self.at += 1;
                    hi = match e { 'x' => self.hex()?, 't' => '\t', 'n' => '\n', 'r' => '\r', e => e };
                }
                if hi < lo { return self.err("reversed range"); }
                items.push(Item::Range(lo, hi));
            } else {
                items.push(Item::Range(lo, lo));
            }
        }
        Ok(Set { negated, items })
    }
}

static NAMES: &[u8] = include_bytes!("unicode_names.bin");

/// The character Unicode gives `name` (`\N{...}`; any letter case). The table (`next/tools/gen-unicode-names.py`) holds the names that are
/// not made by a rule; Hangul syllables and CJK ideographs are worked out.
pub fn character_named(name: &str) -> Option<char> {
    static TABLE: OnceLock<HashMap<String, char>> = OnceLock::new();
    let key = name.to_ascii_uppercase();
    if let Some(hex) = key.strip_prefix("CJK UNIFIED IDEOGRAPH-") {
        let cp = u32::from_str_radix(hex, 16).ok()?;
        let ranges = [(0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0x20000, 0x2A6DF), (0x2A700, 0x2B739), (0x2B740, 0x2B81D), (0x2B820, 0x2CEA1), (0x2CEB0, 0x2EBE0), (0x30000, 0x3134A), (0x31350, 0x323AF)];
        return if hex.len() >= 4 && ranges.iter().any(|(a, b)| (*a..=*b).contains(&cp)) { char::from_u32(cp) } else { None };
    }
    if let Some(rest) = key.strip_prefix("HANGUL SYLLABLE ") { return hangul_syllable(rest); }
    TABLE.get_or_init(|| {
        let le32 = |at: usize| u32::from_le_bytes([NAMES[at], NAMES[at + 1], NAMES[at + 2], NAMES[at + 3]]) as usize;
        let mut at = 4;
        let mut words = Vec::with_capacity(le32(0));
        for _ in 0..le32(0) {
            let n = NAMES[at] as usize;
            words.push(std::str::from_utf8(&NAMES[at + 1..at + 1 + n]).unwrap_or(""));
            at += 1 + n;
        }
        let count = le32(at);
        at += 4;
        let mut table = HashMap::with_capacity(count);
        for _ in 0..count {
            let cp = le32(at) as u32;
            let n = NAMES[at + 4] as usize;
            at += 5;
            let mut full = String::new();
            for k in 0..n {
                if k > 0 { full.push(' '); }
                full.push_str(words[u16::from_le_bytes([NAMES[at], NAMES[at + 1]]) as usize]);
                at += 2;
            }
            if let Some(c) = char::from_u32(cp) { table.insert(full, c); }
        }
        table
    }).get(&key).copied()
}

/// `HANGUL SYLLABLE GA`: the Unicode rule (initial, vowel and final jamo names).
fn hangul_syllable(rest: &str) -> Option<char> {
    const L: [&str; 19] = ["G", "GG", "N", "D", "DD", "R", "M", "B", "BB", "S", "SS", "", "J", "JJ", "C", "K", "T", "P", "H"];
    const V: [&str; 21] = ["A", "AE", "YA", "YAE", "EO", "E", "YEO", "YE", "O", "WA", "WAE", "OE", "YO", "U", "WEO", "WE", "WI", "YU", "EU", "YI", "I"];
    const T: [&str; 28] = ["", "G", "GG", "GS", "N", "NJ", "NH", "D", "L", "LG", "LM", "LB", "LS", "LT", "LP", "LH", "M", "B", "BS", "S", "SS", "NG", "J", "C", "K", "T", "P", "H"];
    for (l, lead) in L.iter().enumerate() {
        let Some(after_lead) = rest.strip_prefix(lead) else { continue };
        for (v, vowel) in V.iter().enumerate() {
            let Some(after_vowel) = after_lead.strip_prefix(vowel) else { continue };
            if let Some(t) = T.iter().position(|tail| *tail == after_vowel) { return char::from_u32(0xAC00 + ((l * 21 + v) * 28 + t) as u32); }
        }
    }
    None
}

/// The stand-ins (`folio_model::marker`) of the character InDesign gives a marker in `Story.contents`.
fn marker_of_code(c: char) -> &'static [char] {
    match c {
        '\u{18}' => &[marker::PAGE, marker::NEXT_PAGE, marker::PREVIOUS_PAGE, marker::VARIABLE],
        '\u{19}' => &[marker::SECTION],
        '\u{4}' => &[marker::NOTE],
        '\u{3}' => &[marker::END_NESTED],
        _ => &[],
    }
}

fn code_node(code: char) -> Node {
    let chars: Vec<char> = CODES.iter().find(|(k, _)| *k == code).expect("caller checked membership in CODES").1.chars().collect();
    if chars.len() == 1 { Node::Char(chars[0]) } else { Node::Set(Set { negated: false, items: chars.into_iter().map(|c| Item::Range(c, c)).collect() }) }
}

fn fixed_length(n: &Node) -> Option<usize> {
    Some(match n {
        Node::Empty | Node::Bol | Node::Eol | Node::WordEdge(_) | Node::WordStart | Node::WordEnd | Node::Start | Node::End | Node::Contiguous | Node::Look { .. } => 0,
        Node::Char(_) | Node::Any | Node::Set(_) | Node::Break(_) => 1,
        Node::Seq(v) => v.iter().map(fixed_length).sum::<Option<usize>>()?,
        Node::Alt(v) => { let first = fixed_length(v.first()?)?; if v.iter().all(|a| fixed_length(a) == Some(first)) { first } else { return None } }
        Node::Repeat { node, min, max, .. } if min == max => fixed_length(node)? * *min as usize,
        Node::Repeat { .. } | Node::BackRef(_) => return None,
        Node::Group(b, _) | Node::Atomic(b) | Node::Flags(b, _, _) => fixed_length(b)?,
    })
}

// ---------- matching

struct Cx<'t> { text: &'t [char], caps: Vec<Option<(usize, usize)>>, last_end: usize, ci: bool, dotall: bool, first: bool, term: ParagraphStart }

fn is_word(c: char) -> bool { pred(Pred::Word, c) }

type Cont<'k, 't> = &'k mut dyn FnMut(usize, &mut Cx<'t>) -> bool;

impl Regex {
    /// Compile `pattern`. The error says which part InDesign would not take or Folio does not model.
    pub fn new(pattern: &str) -> Result<Regex, String> {
        let mut p = Parser { p: pattern.chars().collect(), at: 0, groups: 0, names: Vec::new(), flags_used: false, end_nested: false, discretionary_break: false, index: false };
        let mut flags = Flags { ci: false, dotall: false, ext: false };
        let root = p.alt(&mut flags)?;
        if p.at < p.p.len() { return p.err("unbalanced )"); }
        let names_indent_tab = names_tab(&root);
        Ok(Regex { root, groups: p.groups, names_end_nested: p.end_nested, names_discretionary_break: p.discretionary_break, names_index: p.index, names_indent_tab })
    }

    /// The styled ranges (character indexes into `text`). InDesign scans left to right and stops at the first empty match.
    /// `first`: this is the story's first paragraph (`\A` is the start of the story, not of the paragraph: measured).
    pub fn find_all(&self, text: &[char], first: bool) -> Vec<Range<usize>> { self.find_all_in(text, first, ParagraphStart::Anywhere) }

    /// The expression names the End Nested Style Character (`~h`, `\x{0003}`): the invisible character has to be in the text it is matched
    /// against. Without that, GREP does not see it (measured: it is transparent to `.`, `\w+`, `a.b`; rules/specialchars-c1.ko.md).
    pub fn names_end_nested(&self) -> bool { self.names_end_nested }

    /// The expression names the discretionary line break (`~k`, `\x{200b}`). Without that, GREP does not see it (measured: `.` `\W` `[^a]`
    /// pass over it, `a.b` does not match across it).
    pub fn names_discretionary_break(&self) -> bool { self.names_discretionary_break }

    /// The expression names the index marker (`~I`). Without that, GREP does not see it (measured: transparent like the discretionary line break).
    pub fn names_index(&self) -> bool { self.names_index }

    /// Something in the expression can match the indent-to-here tab (`\s`, `~i`, `\x{0007}`). Without that, GREP does not see the tab (measured:
    /// every other class, `.` and a negated set pass over it, `ab` matches across it).
    pub fn names_indent_tab(&self) -> bool { self.names_indent_tab }

    /// `find_all` for a paragraph whose return is the break character `term` (`~M ~R ~P ~L ~E` find it; every other expression takes it as a return).
    pub fn find_all_in(&self, text: &[char], first: bool, term: ParagraphStart) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        let mut pos = 0;
        let mut cx = Cx { text, caps: vec![None; self.groups + 1], last_end: 0, ci: false, dotall: false, first, term };
        while pos <= text.len() {
            let mut found = None;
            for start in pos..=text.len() {
                cx.caps.iter_mut().for_each(|c| *c = None);
                cx.last_end = pos;
                let mut end = start;
                if self.m(&self.root, start, &mut cx, &mut |j, _| { end = j; true }) { found = Some((start, end)); break; }
            }
            match found {
                Some((s, e)) if e > s => { out.push(s..e); pos = e; }
                _ => break,
            }
        }
        out
    }

    fn single(&self, n: &Node, i: usize, cx: &Cx<'_>) -> Option<bool> {
        let c = *cx.text.get(i)?;
        // The index marker is found by `~I` alone. The indent-to-here tab is found by a literal U+0007 and by `\s` alone: not by `.`, `\S`, `\W`,
        // `\H`, `\h` or a negated set (measured, rules/specialchars-c1.ko.md).
        if c == marker::INDEX {
            return match n { Node::Char(x) => Some(*x == c), Node::Any | Node::Set(_) | Node::Break(_) => Some(false), _ => None };
        }
        if c == INDENT_TAB {
            return match n { Node::Char(x) => Some(*x == c), Node::Set(s) => Some(set_names_tab(s)), Node::Any | Node::Break(_) => Some(false), _ => None };
        }
        Some(match n {
            Node::Char(x) => *x == c || (cx.ci && (variants(*x).contains(&c) || variants(c).contains(x))),
            Node::Any => c != '\u{ad}' && (cx.dotall || !matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')),
            Node::Set(s) => s.contains(c, cx.ci),
            Node::Break(kind) => c == '\r' && i + 1 == cx.text.len() && cx.term == *kind,
            _ => return None,
        })
    }

    fn m<'t>(&self, n: &Node, i: usize, cx: &mut Cx<'t>, k: Cont<'_, 't>) -> bool {
        match n {
            Node::Empty => k(i, cx),
            Node::Char(_) | Node::Any | Node::Set(_) | Node::Break(_) => self.single(n, i, cx) == Some(true) && k(i + 1, cx),
            Node::Seq(v) => self.seq(v, i, cx, k),
            Node::Alt(v) => v.iter().any(|a| self.m(a, i, cx, k)),
            Node::Flags(b, ci, dotall) => {
                let (oc, od) = (cx.ci, cx.dotall);
                cx.ci = *ci; cx.dotall = *dotall;
                let r = self.m(b, i, cx, &mut |j, cx| { let (c2, d2) = (cx.ci, cx.dotall); cx.ci = oc; cx.dotall = od; let r = k(j, cx); cx.ci = c2; cx.dotall = d2; r });
                cx.ci = oc; cx.dotall = od;
                r
            }
            Node::Group(b, Some(idx)) => {
                let old = cx.caps[*idx];
                let r = self.m(b, i, cx, &mut |j, cx| {
                    let prev = cx.caps[*idx];
                    cx.caps[*idx] = Some((i, j));
                    if k(j, cx) { true } else { cx.caps[*idx] = prev; false }
                });
                if !r { cx.caps[*idx] = old; }
                r
            }
            Node::Group(b, None) => self.m(b, i, cx, k),
            Node::Atomic(b) => {
                let mut end = None;
                if self.m(b, i, cx, &mut |j, _| { end = Some(j); true }) { k(end.expect("successful match invoked the continuation that records its end"), cx) } else { false }
            }
            Node::Look { node, ahead, negative, length } => {
                let saved = cx.caps.clone();
                let ok = if *ahead { self.m(node, i, cx, &mut |_, _| true) } else if i >= *length { self.m(node, i - length, cx, &mut |j, _| j == i) } else { false };
                if *negative { cx.caps = saved; !ok && k(i, cx) } else { ok && k(i, cx) }
            }
            Node::BackRef(idx) => match cx.caps[*idx] {
                Some((s, e)) => {
                    let len = e - s;
                    if i + len > cx.text.len() { return false; }
                    let same = (0..len).all(|d| { let (a, b) = (cx.text[s + d], cx.text[i + d]); a == b || (cx.ci && variants(a).contains(&b)) });
                    same && k(i + len, cx)
                }
                None => false,
            },
            Node::Bol => (i == 0 || cx.text[i - 1] == '\n') && k(i, cx),
            Node::Eol => (i == cx.text.len() || matches!(cx.text[i], '\n' | '\r')) && k(i, cx),
            Node::Start => i == 0 && cx.first && k(i, cx),
            Node::End => i == cx.text.len() && k(i, cx),
            Node::Contiguous => i == cx.last_end && k(i, cx),
            Node::WordEdge(want) => {
                let before = i > 0 && is_word(cx.text[i - 1]);
                let after = i < cx.text.len() && is_word(cx.text[i]);
                ((before != after) == *want) && k(i, cx)
            }
            Node::WordStart => {
                let before = i > 0 && is_word(cx.text[i - 1]);
                let after = i < cx.text.len() && is_word(cx.text[i]);
                !before && after && k(i, cx)
            }
            Node::WordEnd => {
                let before = i > 0 && is_word(cx.text[i - 1]);
                let after = i < cx.text.len() && is_word(cx.text[i]);
                before && !after && k(i, cx)
            }
            Node::Repeat { node, min, max, kind } => {
                if matches!(**node, Node::Char(_) | Node::Any | Node::Set(_) | Node::Break(_)) {
                    let mut n = 0u32;
                    while n < *max && self.single(node, i + n as usize, cx) == Some(true) { n += 1; }
                    if n < *min { return false; }
                    return match kind {
                        Kind::Greedy => (*min..=n).rev().any(|c| k(i + c as usize, cx)),
                        Kind::Lazy => (*min..=n).any(|c| k(i + c as usize, cx)),
                        Kind::Possessive => k(i + n as usize, cx),
                    };
                }
                if *kind == Kind::Possessive {
                    let mut end = None;
                    if self.rep(node, *min, *max, Kind::Greedy, 0, i, cx, &mut |j, _| { end = Some(j); true }) { return k(end.expect("successful match invoked the continuation that records its end"), cx); }
                    return false;
                }
                self.rep(node, *min, *max, *kind, 0, i, cx, k)
            }
        }
    }

    fn seq<'t>(&self, v: &[Node], i: usize, cx: &mut Cx<'t>, k: Cont<'_, 't>) -> bool {
        match v.split_first() {
            None => k(i, cx),
            Some((h, t)) => self.m(h, i, cx, &mut |j, cx| self.seq(t, j, cx, k)),
        }
    }

    // Keep the recursive matcher state and continuation explicit at each call.
    #[allow(clippy::too_many_arguments)]
    fn rep<'t>(&self, node: &Node, min: u32, max: u32, kind: Kind, count: u32, i: usize, cx: &mut Cx<'t>, k: Cont<'_, 't>) -> bool {
        if kind == Kind::Lazy {
            if count >= min && k(i, cx) { return true; }
            self.rep_more(node, min, max, kind, count, i, cx, k)
        } else {
            if self.rep_more(node, min, max, kind, count, i, cx, k) { return true; }
            count >= min && k(i, cx)
        }
    }

    // Keep the recursive matcher state and continuation explicit at each call.
    #[allow(clippy::too_many_arguments)]
    fn rep_more<'t>(&self, node: &Node, min: u32, max: u32, kind: Kind, count: u32, i: usize, cx: &mut Cx<'t>, k: Cont<'_, 't>) -> bool {
        count < max && self.m(node, i, cx, &mut |j, cx| {
            if j == i { return count + 1 >= min && k(j, cx); }
            self.rep(node, min, max, kind, count + 1, j, cx, k)
        })
    }
}
