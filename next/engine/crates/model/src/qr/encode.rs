use super::{QrModules, mask};
use qrcodegen::{Mask, QrCode, QrCodeEcc, QrSegment, Version};

const ALPHANUMERIC: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mode {
    Numeric,
    Alphanumeric,
    Byte,
}

impl Mode {
    fn count_bits(self, group: usize) -> usize {
        match self {
            Self::Numeric => [10, 12, 14][group],
            Self::Alphanumeric => [9, 11, 13][group],
            Self::Byte => [8, 16, 16][group],
        }
    }

    fn accepts(self, byte: u8) -> bool {
        match self {
            Self::Numeric => byte.is_ascii_digit(),
            Self::Alphanumeric => ALPHANUMERIC.contains(&byte),
            Self::Byte => true,
        }
    }

    fn data_bits(self, length: usize) -> usize {
        match self {
            Self::Numeric => (length * 10).div_ceil(3),
            Self::Alphanumeric => (length * 11).div_ceil(2),
            Self::Byte => length * 8,
        }
    }

    fn segment(self, text: &str) -> QrSegment {
        match self {
            Self::Numeric => QrSegment::make_numeric(text),
            Self::Alphanumeric => QrSegment::make_alphanumeric(text),
            Self::Byte => QrSegment::make_bytes(text.as_bytes()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Choice {
    bits: usize,
    count: usize,
    mode: Mode,
    end: usize,
}

// R5
fn segments(text: &str, starts: &[usize], group: usize) -> Vec<QrSegment> {
    if text.is_empty() {
        return vec![Mode::Byte.segment(text)];
    }
    let length = starts.len() - 1;
    let mut best = vec![
        Choice {
            bits: 0,
            count: 0,
            mode: Mode::Byte,
            end: length
        };
        length + 1
    ];
    for start in (0..length).rev() {
        let mut winner = Choice {
            bits: usize::MAX,
            ..best[length]
        };
        // R5: a fixed first mode and end share the same optimal suffix.
        for mode in [Mode::Numeric, Mode::Alphanumeric, Mode::Byte] {
            let count_bits = mode.count_bits(group);
            for end in start + 1..=length {
                let count = if mode == Mode::Byte {
                    starts[end] - starts[start]
                } else {
                    end - start
                };
                if count >= 1 << count_bits || !mode.accepts(text.as_bytes()[starts[end - 1]]) {
                    break;
                }
                let choice = Choice {
                    bits: 4 + count_bits + mode.data_bits(count) + best[end].bits,
                    count: 1 + best[end].count,
                    mode,
                    end,
                };
                winner = winner.min(choice);
            }
        }
        best[start] = winner;
    }
    let mut result = Vec::with_capacity(best[0].count);
    let mut start = 0;
    while start < length {
        let choice = best[start];
        result.push(
            choice
                .mode
                .segment(&text[starts[start]..starts[choice.end]]),
        );
        start = choice.end;
    }
    result
}

fn modules(code: &QrCode) -> QrModules {
    let size = code.size();
    let dark = (0..size)
        .flat_map(|y| (0..size).map(move |x| code.get_module(x, y)))
        .collect();
    QrModules {
        size: size as usize,
        dark,
    }
}

// R4
pub fn encode(payload: &str) -> Option<QrModules> {
    if payload.len() > 5596 {
        return None;
    }
    let starts: Vec<_> = payload
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(payload.len()))
        .collect();
    let ecc = QrCodeEcc::Medium;
    for (group, (first, last)) in [(1, 9), (10, 26), (27, 40)].into_iter().enumerate() {
        let data = segments(payload, &starts, group);
        let Ok(initial) = QrCode::encode_segments_advanced(
            &data,
            ecc,
            Version::new(first),
            Version::new(last),
            Some(Mask::new(0)),
            false,
        ) else {
            continue;
        };
        let version = initial.version();
        let mut candidates = vec![modules(&initial)];
        for id in 1..8 {
            let code = QrCode::encode_segments_advanced(
                &data,
                ecc,
                version,
                version,
                Some(Mask::new(id)),
                false,
            )
            .ok()?;
            candidates.push(modules(&code));
        }
        return mask::select(candidates);
    }
    None
}
