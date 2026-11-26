//! Keyboard shortcut of a paragraph or character style: the IDML pair `KeyboardShortcut="<modifiers> <vk>"` and
//! `ExtendedKeyboardShortcut="<modifiers> <key> <flag>"`.
//!
//! Measured in InDesign 2025 (Windows) with real key input into the Style Options dialog's "Shortcut" field, then an IDML export
//! (artifacts/stylekeys-c1-20261023, rules/stylekeys.ko.md). The rules:
//! - modifiers are a sum: Ctrl 1, Shift 16, Alt 256 (both attributes).
//! - the keypad digits Num0..Num9 use the short attribute (`mods 96+digit`, the extended one stays `0 0 0`). Shift is dropped for them
//!   (Ctrl+Shift+Num1 is stored and shown as Ctrl+Num1).
//! - every other key uses the extended attribute (the short one stays `0 0`): F1..F12 are `n+6` (flag 0); the keypad operators
//!   `* + - . /` are `0xC0000000 | ascii` as a signed 32-bit integer (flag 0); letters, main-row digits and the punctuation keys
//!   `; = , - . / ` [ \ ] '` are the ascii code (flag 1); Space is 1 and the cursor keys `0x40000000 | n` (flag 0): Home 0x24, End 0x25,
//!   Page Up 0x26, Page Down 0x27, Left 0x2A, Right 0x2B, Up 0x2C, Down 0x2D (stylekeys-c2, measured).
//! - which modifier sets the field takes (stylekeys-c2, `rules/stylekeys-c2.ko.md`): Ctrl with anything; Alt together with Shift; Shift alone
//!   only with a function key or a cursor key (Shift+Space is shown but not kept; Shift with a letter, digit, punctuation or keypad digit is
//!   refused). Alt alone and a bare key are refused (and InDesign still keeps a bare key as an inert record, which this module reads as
//!   "no shortcut"). Insert, Delete and Backspace are refused with any modifiers; the field itself eats Ctrl+A, C, V, X, Z and Ctrl+Shift+Z.
//!
//! Folio spells a shortcut as `ctrl+alt+num1`, `shift+ctrl+f2`, `ctrl+alt+a`, `ctrl+=` (modifiers in any order, key last; keypad operators
//! `numadd` `nummul` `numsub` `numdot`; punctuation as the character; `space`, `left` `right` `up` `down` `home` `end` `pageup` `pagedown`).
use std::fmt;

const CTRL: i64 = 1;
const SHIFT: i64 = 16;
const ALT: i64 = 256;
const KEYPAD_CHAR: i64 = 0xC000_0000;
/// Cursor keys: `0x40000000 | n` (extended attribute, flag 0).
const CURSOR: i64 = 0x4000_0000;
/// What an error of a key the shortcut field takes for itself starts with (Ctrl+A, C, V, X, Z: the field keeps what it showed).
pub const RESERVED: &str = "reserved";
/// `KeyboardShortcut` and `ExtendedKeyboardShortcut` of a style without a shortcut.
pub const NONE: (&str, &str) = ("0 0", "0 0 0");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// keypad digit 0..=9
    Num(u8),
    /// keypad `*`, `+`, `-`, `.` or `/`
    NumOp(char),
    /// function key 1..=12
    F(u8),
    /// `A`..`Z`, `0`..`9` or one of `; = , - . / ` [ \ ] '` on the main keyboard (letters upper case)
    Char(char),
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// The punctuation keys of the main keyboard a style shortcut can use (measured: all accepted, stored as their ascii code).
const PUNCTUATION: &str = ";=,-./`[\\]'";

impl Key {
    fn cursor(self) -> Option<i64> {
        Some(match self {
            Key::Home => 0x24,
            Key::End => 0x25,
            Key::PageUp => 0x26,
            Key::PageDown => 0x27,
            Key::Left => 0x2A,
            Key::Right => 0x2B,
            Key::Up => 0x2C,
            Key::Down => 0x2D,
            _ => return None,
        })
    }

    fn from_cursor(n: i64) -> Option<Key> {
        Some(match n {
            0x24 => Key::Home,
            0x25 => Key::End,
            0x26 => Key::PageUp,
            0x27 => Key::PageDown,
            0x2A => Key::Left,
            0x2B => Key::Right,
            0x2C => Key::Up,
            0x2D => Key::Down,
            _ => return None,
        })
    }

    /// A function key or a cursor key: Shift alone is enough for them.
    fn shift_alone_ok(self) -> bool {
        matches!(self, Key::F(_)) || self.cursor().is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Shortcut {
    /// Builds a shortcut: the Shift of a keypad digit goes away (measured) and a shortcut without a usable modifier is refused.
    pub fn new(ctrl: bool, shift: bool, alt: bool, key: Key) -> Result<Self, String> {
        let shift = shift && !matches!(key, Key::Num(_));
        let valid_key = match key {
            Key::Num(d) => d <= 9,
            Key::NumOp(c) => matches!(c, '*' | '+' | '-' | '.' | '/'),
            Key::F(n) => (1..=12).contains(&n),
            Key::Char(c) => c.is_ascii_uppercase() || c.is_ascii_digit() || PUNCTUATION.contains(c),
            _ => true,
        };
        if !valid_key { return Err("unsupported key".into()); }
        // measured (stylekeys-c2): Ctrl takes any key; Alt needs Shift or Ctrl with it; Shift alone only takes function and cursor keys
        let ok = ctrl || alt && shift || shift && key.shift_alone_ok();
        if !ok {
            return Err("a style shortcut needs Ctrl, or Alt with Shift (function and cursor keys also take Shift alone)".into());
        }
        Ok(Self { ctrl, shift, alt, key })
    }

    /// `ctrl+alt+num1`: modifiers in any order, the key last, case does not matter.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let lower = spec.trim().to_ascii_lowercase();
        let mut parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let key = parts.pop().filter(|k| !k.is_empty()).ok_or("shortcut has no key")?;
        let (mut ctrl, mut shift, mut alt) = (false, false, false);
        for m in parts {
            match m {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" => alt = true,
                _ => return Err(format!("unknown modifier {m}")),
            }
        }
        let key = match key {
            "space" => Key::Space,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "nummul" => Key::NumOp('*'),
            "numadd" => Key::NumOp('+'),
            "numsub" => Key::NumOp('-'),
            "numdot" => Key::NumOp('.'),
            "numdiv" => Key::NumOp('/'),
            k if k.len() == 4 && k.starts_with("num") && k.as_bytes()[3].is_ascii_digit() => Key::Num(k.as_bytes()[3] - b'0'),
            k if k.starts_with('f') && k.len() > 1 && k[1..].chars().all(|c| c.is_ascii_digit()) => Key::F(k[1..].parse().map_err(|_| "bad function key")?),
            k if k.chars().count() == 1 => Key::Char(k.chars().next().expect("guard checked exactly one character").to_ascii_uppercase()),
            k => return Err(format!("unknown key {k}")),
        };
        let s = Self::new(ctrl, shift, alt, key)?;
        // the field itself takes these (select all, copy, paste, cut, undo, redo): InDesign shows no shortcut for them (measured)
        if let (true, false, Key::Char(c)) = (s.ctrl, s.alt, s.key)
            && ((!s.shift && matches!(c, 'A' | 'C' | 'V' | 'X' | 'Z')) || (s.shift && c == 'Z')) {
            return Err(format!("{RESERVED}: the shortcut field uses this key itself"));
        }
        Ok(s)
    }

    /// The canonical spelling that `parse` reads back (modifier order ctrl, shift, alt).
    pub fn spec(&self) -> String {
        let mut s = String::new();
        for (on, name) in [(self.ctrl, "ctrl+"), (self.shift, "shift+"), (self.alt, "alt+")] {
            if on { s += name; }
        }
        s += &match self.key {
            Key::Num(d) => format!("num{d}"),
            Key::NumOp('*') => "nummul".into(),
            Key::NumOp('+') => "numadd".into(),
            Key::NumOp('-') => "numsub".into(),
            Key::NumOp('/') => "numdiv".into(),
            Key::NumOp(_) => "numdot".into(),
            Key::F(n) => format!("f{n}"),
            Key::Char(c) => c.to_ascii_lowercase().to_string(),
            Key::Space => "space".into(),
            Key::Left => "left".into(),
            Key::Right => "right".into(),
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Home => "home".into(),
            Key::End => "end".into(),
            Key::PageUp => "pageup".into(),
            Key::PageDown => "pagedown".into(),
        };
        s
    }

    /// What InDesign's field shows: `Shift+Ctrl+Alt+F2`, `Ctrl+Alt+Num 1`, `Ctrl+Alt+Left Arrow` (the order is Shift, Ctrl, Alt).
    pub fn label(&self) -> String {
        let mut s = String::new();
        for (on, name) in [(self.shift, "Shift+"), (self.ctrl, "Ctrl+"), (self.alt, "Alt+")] {
            if on { s += name; }
        }
        s += &match self.key {
            Key::Num(d) => format!("Num {d}"),
            Key::NumOp(c) => format!("Num {c}"),
            Key::F(n) => format!("F{n}"),
            Key::Char(c) => c.to_string(),
            Key::Space => "Space".into(),
            Key::Left => "Left Arrow".into(),
            Key::Right => "Right Arrow".into(),
            Key::Up => "Up Arrow".into(),
            Key::Down => "Down Arrow".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "Page Up".into(),
            Key::PageDown => "Page Down".into(),
        };
        s
    }

    fn mods(&self) -> i64 {
        i64::from(self.ctrl) * CTRL + i64::from(self.shift) * SHIFT + i64::from(self.alt) * ALT
    }

    /// `(KeyboardShortcut, ExtendedKeyboardShortcut)`
    pub fn to_idml(&self) -> (String, String) {
        let m = self.mods();
        let ext = |key: i64, flag: i64| (NONE.0.to_owned(), format!("{m} {key} {flag}"));
        match self.key {
            Key::Num(d) => (format!("{m} {}", 96 + i64::from(d)), NONE.1.to_owned()),
            Key::NumOp(c) => ext(i64::from(((KEYPAD_CHAR | c as i64) as u32) as i32), 0),
            Key::F(n) => ext(i64::from(n) + 6, 0),
            Key::Char(c) => ext(c as i64, 1),
            Key::Space => ext(1, 0),
            k => ext(CURSOR | k.cursor().unwrap_or(0), 0),
        }
    }

    /// Reads the two attributes. `None`: no shortcut, or a record that is not a usable shortcut (a bare key, an unknown key code).
    pub fn from_idml(short: &str, extended: &str) -> Option<Self> {
        let nums = |s: &str| s.split_whitespace().map(str::parse::<i64>).collect::<Result<Vec<_>, _>>().ok();
        let short = nums(short)?;
        let extended = nums(extended).unwrap_or_default();
        let (mods, key) = if short.len() == 2 && short[1] != 0 {
            (short[0], match short[1] {
                96..=105 => Key::Num((short[1] - 96) as u8),
                _ => return None,
            })
        } else if extended.len() == 3 {
            let (flag, code) = (extended[2], extended[1]);
            (extended[0], match (flag, code) {
                (0, 7..=18) => Key::F((code - 6) as u8),
                (0, 1) => Key::Space,
                (0, c) if c & !0xFF == CURSOR => Key::from_cursor(c & 0xFF)?,
                (0, c) if (i64::from(c as i32) as u32) & 0xC000_0000 == 0xC000_0000 => {
                    let ch = char::from_u32((c as i32 as u32) & 0xFF)?;
                    if !matches!(ch, '*' | '+' | '-' | '.' | '/') { return None; }
                    Key::NumOp(ch)
                }
                (1, c) => {
                    let ch = char::from_u32(u32::try_from(c).ok()?)?;
                    if !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || PUNCTUATION.contains(ch)) { return None; }
                    Key::Char(ch)
                }
                _ => return None,
            })
        } else {
            return None;
        };
        if mods & !(CTRL | SHIFT | ALT) != 0 { return None; }
        Self::new(mods & CTRL != 0, mods & SHIFT != 0, mods & ALT != 0, key).ok()
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.label()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // every row is a real measurement: key sent into the dialog, then the style's attributes in the exported IDML
    const MEASURED: &[(&str, &str, &str, &str)] = &[
        ("ctrl+num1", "1 97", "0 0 0", "Ctrl+Num 1"),
        ("ctrl+num0", "1 96", "0 0 0", "Ctrl+Num 0"),
        ("ctrl+num9", "1 105", "0 0 0", "Ctrl+Num 9"),
        ("ctrl+num3", "1 99", "0 0 0", "Ctrl+Num 3"),
        ("ctrl+alt+num1", "257 97", "0 0 0", "Ctrl+Alt+Num 1"),
        ("ctrl+alt+num5", "257 101", "0 0 0", "Ctrl+Alt+Num 5"),
        ("ctrl+nummul", "0 0", "1 -1073741782 0", "Ctrl+Num *"),
        ("ctrl+numadd", "0 0", "1 -1073741781 0", "Ctrl+Num +"),
        ("ctrl+numsub", "0 0", "1 -1073741779 0", "Ctrl+Num -"),
        ("ctrl+alt+numdot", "0 0", "257 -1073741778 0", "Ctrl+Alt+Num ."),
        ("ctrl+shift+alt+numadd", "0 0", "273 -1073741781 0", "Shift+Ctrl+Alt+Num +"),
        ("ctrl+alt+f1", "0 0", "257 7 0", "Ctrl+Alt+F1"),
        ("ctrl+alt+f2", "0 0", "257 8 0", "Ctrl+Alt+F2"),
        ("ctrl+alt+f4", "0 0", "257 10 0", "Ctrl+Alt+F4"),
        ("shift+f2", "0 0", "16 8 0", "Shift+F2"),
        ("shift+f5", "0 0", "16 11 0", "Shift+F5"),
        ("ctrl+shift+f3", "0 0", "17 9 0", "Shift+Ctrl+F3"),
        ("ctrl+shift+alt+f2", "0 0", "273 8 0", "Shift+Ctrl+Alt+F2"),
        ("shift+alt+f2", "0 0", "272 8 0", "Shift+Alt+F2"),
        ("ctrl+alt+a", "0 0", "257 65 1", "Ctrl+Alt+A"),
        ("ctrl+alt+z", "0 0", "257 90 1", "Ctrl+Alt+Z"),
        ("ctrl+alt+1", "0 0", "257 49 1", "Ctrl+Alt+1"),
        ("ctrl+alt+0", "0 0", "257 48 1", "Ctrl+Alt+0"),
        ("ctrl+alt+/", "0 0", "257 47 1", "Ctrl+Alt+/"),
        ("ctrl+alt+space", "0 0", "257 1 0", "Ctrl+Alt+Space"),
        ("ctrl+alt+left", "0 0", "257 1073741866 0", "Ctrl+Alt+Left Arrow"),
        // stylekeys-c2: punctuation, cursor keys, more function keys and plain Ctrl+letter (e1 session)
        ("ctrl+;", "0 0", "1 59 1", "Ctrl+;"),
        ("ctrl+=", "0 0", "1 61 1", "Ctrl+="),
        ("ctrl+,", "0 0", "1 44 1", "Ctrl+,"),
        ("ctrl+-", "0 0", "1 45 1", "Ctrl+-"),
        ("ctrl+.", "0 0", "1 46 1", "Ctrl+."),
        ("ctrl+/", "0 0", "1 47 1", "Ctrl+/"),
        ("ctrl+`", "0 0", "1 96 1", "Ctrl+`"),
        ("ctrl+[", "0 0", "1 91 1", "Ctrl+["),
        ("ctrl+]", "0 0", "1 93 1", "Ctrl+]"),
        ("ctrl+'", "0 0", "1 39 1", "Ctrl+'"),
        ("ctrl+up", "0 0", "1 1073741868 0", "Ctrl+Up Arrow"),
        ("ctrl+down", "0 0", "1 1073741869 0", "Ctrl+Down Arrow"),
        ("ctrl+left", "0 0", "1 1073741866 0", "Ctrl+Left Arrow"),
        ("ctrl+right", "0 0", "1 1073741867 0", "Ctrl+Right Arrow"),
        ("ctrl+home", "0 0", "1 1073741860 0", "Ctrl+Home"),
        ("ctrl+end", "0 0", "1 1073741861 0", "Ctrl+End"),
        ("ctrl+pageup", "0 0", "1 1073741862 0", "Ctrl+Page Up"),
        ("ctrl+pagedown", "0 0", "1 1073741863 0", "Ctrl+Page Down"),
        ("ctrl+shift+up", "0 0", "17 1073741868 0", "Shift+Ctrl+Up Arrow"),
        ("ctrl+alt+home", "0 0", "257 1073741860 0", "Ctrl+Alt+Home"),
        ("ctrl+f6", "0 0", "1 12 0", "Ctrl+F6"),
        ("ctrl+f9", "0 0", "1 15 0", "Ctrl+F9"),
        ("ctrl+f10", "0 0", "1 16 0", "Ctrl+F10"),
        ("ctrl+f11", "0 0", "1 17 0", "Ctrl+F11"),
        ("shift+f11", "0 0", "16 17 0", "Shift+F11"),
        ("ctrl+f12", "0 0", "1 18 0", "Ctrl+F12"),
        ("shift+f12", "0 0", "16 18 0", "Shift+F12"),
        ("ctrl+shift+f8", "0 0", "17 14 0", "Shift+Ctrl+F8"),
        ("ctrl+n", "0 0", "1 78 1", "Ctrl+N"),
        ("ctrl+shift+p", "0 0", "17 80 1", "Shift+Ctrl+P"),
        ("alt+shift+p", "0 0", "272 80 1", "Shift+Alt+P"),
        ("ctrl+alt+shift+h", "0 0", "273 72 1", "Shift+Ctrl+Alt+H"),
        ("ctrl+0", "0 0", "1 48 1", "Ctrl+0"),
        ("ctrl+shift+1", "0 0", "17 49 1", "Shift+Ctrl+1"),
        // e2 session: Shift alone with cursor keys is kept, Alt+Shift with keypad operators and cursor keys too
        ("shift+up", "0 0", "16 1073741868 0", "Shift+Up Arrow"),
        ("shift+home", "0 0", "16 1073741860 0", "Shift+Home"),
        ("shift+pageup", "0 0", "16 1073741862 0", "Shift+Page Up"),
        ("shift+left", "0 0", "16 1073741866 0", "Shift+Left Arrow"),
        ("alt+shift+numadd", "0 0", "272 -1073741781 0", "Shift+Alt+Num +"),
        ("alt+shift+left", "0 0", "272 1073741866 0", "Shift+Alt+Left Arrow"),
        ("alt+shift+f5", "0 0", "272 11 0", "Shift+Alt+F5"),
        ("ctrl+alt+shift+numsub", "0 0", "273 -1073741779 0", "Shift+Ctrl+Alt+Num -"),
        // e3 session: the keypad divide key needs the extended-key flag when sent (stylekeys-c1 saw the main-row slash instead)
        ("ctrl+numdiv", "0 0", "1 -1073741777 0", "Ctrl+Num /"),
        ("ctrl+alt+numdiv", "0 0", "257 -1073741777 0", "Ctrl+Alt+Num /"),
        ("alt+shift+numdiv", "0 0", "272 -1073741777 0", "Shift+Alt+Num /"),
        ("ctrl+shift+numdiv", "0 0", "17 -1073741777 0", "Shift+Ctrl+Num /"),
    ];

    #[test]
    fn measured_rows_encode_decode_and_label() {
        for (spec, short, ext, label) in MEASURED {
            let s = Shortcut::parse(spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
            assert_eq!(s.to_idml(), ((*short).to_owned(), (*ext).to_owned()), "{spec}");
            assert_eq!(Shortcut::from_idml(short, ext), Some(s), "{spec}");
            assert_eq!(s.label(), *label, "{spec}");
            assert_eq!(Shortcut::parse(&s.spec()).unwrap(), s, "{spec}");
        }
    }

    #[test]
    fn shift_is_dropped_for_keypad_digits_only() {
        // measured: Ctrl+Shift+Num1 is stored and shown as Ctrl+Num 1, Ctrl+Alt+Shift+Num1 as Ctrl+Alt+Num 1, but Shift stays on a keypad operator
        assert_eq!(Shortcut::parse("ctrl+shift+num1").unwrap().to_idml().0, "1 97");
        assert_eq!(Shortcut::parse("ctrl+alt+shift+num1").unwrap().label(), "Ctrl+Alt+Num 1");
        assert_eq!(Shortcut::parse("ctrl+shift+numadd").unwrap().label(), "Shift+Ctrl+Num +");
    }

    #[test]
    fn bare_and_unusable_keys_are_no_shortcut() {
        // the field stays blank for these (a bare key, Shift with a keypad digit)
        for spec in ["f2", "num1", "a", "shift+num1", "shift+a", "shift+space", "ctrl+f13", "ctrl+nummul+x", "ctrl+", ""] {
            assert!(Shortcut::parse(spec).is_err(), "{spec}");
        }
        // stylekeys-c2: Alt alone never works, Shift alone only with function and cursor keys, and the field keeps a few Ctrl keys for itself
        for spec in ["alt+a", "alt+1", "alt+;", "alt+f2", "alt+up", "alt+home", "alt+num1", "shift+1", "shift+;", "shift+=", "shift+slash", "up", "ctrl+a", "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+z",
            "ctrl+shift+z", "ctrl+insert", "ctrl+delete", "ctrl+backspace", "ctrl+enter", "shift+alt+num1", "shift+numadd", "shift+nummul"] {
            assert!(Shortcut::parse(spec).is_err(), "{spec}");
        }
        for spec in ["shift+f5", "shift+up", "shift+pageup", "shift+home", "alt+shift+p", "alt+shift+up", "alt+shift+f5", "alt+shift+numadd", "ctrl+y", "ctrl+shift+v", "ctrl+shift+a", "ctrl+alt+k", "ctrl+space", "alt+shift+space"] {
            assert!(Shortcut::parse(spec).is_ok(), "{spec}");
        }
        // InDesign keeps a bare 'a' (`0 65 1`) or a bare Num1 (`0 97`) after OK although the field shows nothing: no shortcut
        assert_eq!(Shortcut::from_idml("0 0", "0 65 1"), None);
        assert_eq!(Shortcut::from_idml("0 97", "0 0 0"), None);
        assert_eq!(Shortcut::from_idml("0 0", "0 0 0"), None);
        assert_eq!(Shortcut::from_idml("garbage", "0 0 0"), None);
        assert_eq!(Shortcut::from_idml("0 0", "257 5000 0"), None);
        assert_eq!(Shortcut::from_idml("2 97", "0 0 0"), None);
    }
}
