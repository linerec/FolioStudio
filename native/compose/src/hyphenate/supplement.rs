use std::sync::LazyLock;

struct Bits<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl Bits<'_> {
    fn read(&mut self, width: usize) -> Option<u8> {
        let mut value = 0u8;
        for _ in 0..width {
            let byte = *self.bytes.get(self.offset / 8)?;
            value = (value << 1) | ((byte >> (7 - self.offset % 8)) & 1);
            self.offset += 1;
        }
        Some(value)
    }
}

#[derive(Default)]
struct Words {
    ends: Vec<usize>,
    symbols: Vec<u8>,
}
impl Words {
    fn decode(bytes: &[u8]) -> Option<Self> {
        let header = bytes.strip_prefix(b"FMHY1")?;
        let count = u32::from_le_bytes(header.get(..4)?.try_into().ok()?);
        let mut bits = Bits {
            bytes: header.get(4..)?,
            offset: 0,
        };
        let mut words = Words {
            ends: vec![0],
            symbols: Vec::new(),
        };
        let mut word = Vec::new();
        for _ in 0..count {
            let mut prefix = usize::from(bits.read(4)?);
            if prefix == 15 {
                prefix += usize::from(bits.read(8)?);
            }
            if prefix > word.len() {
                return None;
            }
            word.truncate(prefix);
            loop {
                let symbol = bits.read(6)?;
                if symbol == 0 {
                    break;
                }
                word.push(symbol);
            }
            words.symbols.extend_from_slice(&word);
            words.ends.push(words.symbols.len());
        }
        Some(words)
    }
    fn word(&self, index: usize) -> &[u8] {
        &self.symbols[self.ends[index]..self.ends[index + 1]]
    }
}
pub(super) fn add_points(word: &str, points: &mut Vec<usize>) {
    if !word.is_ascii() || std::env::var_os("FOLIO_NO_HYPH_DICT").is_some_and(|v| v == "1") {
        return;
    }
    static WORDS: LazyLock<Words> = LazyLock::new(|| {
        Words::decode(include_bytes!("../../data/hyph-moby.bin")).unwrap_or_default()
    });
    let count = WORDS.ends.len().saturating_sub(1);
    let mut low = 0;
    let mut high = count;
    while low < high {
        let mid = low + (high - low) / 2;
        let symbols = WORDS.word(mid);
        match symbols
            .iter()
            .map(|&c| b'a' - 1 + (c & 31))
            .cmp(word.bytes())
        {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => {
                points.extend(symbols.iter().enumerate().filter_map(|(i, c)| {
                    let p = i + 1;
                    (c & 32 != 0 && p >= 2 && p + 2 <= symbols.len()).then_some(p)
                }));
                points.sort_unstable();
                points.dedup();
                break;
            }
        }
    }
}
