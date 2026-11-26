//! Non-fatal findings, aggregated by message with counts, in first-seen order.

use std::collections::HashMap;

#[derive(Default)]
pub struct Warnings {
    order: Vec<(String, usize)>,
    index: HashMap<String, usize>,
}

impl Warnings {
    pub fn add(&mut self, msg: impl Into<String>) {
        self.add_n(msg, 1);
    }

    pub fn add_n(&mut self, msg: impl Into<String>, n: usize) {
        if n == 0 {
            return;
        }
        let msg = msg.into();
        if let Some(&i) = self.index.get(&msg) {
            self.order[i].1 += n;
        } else {
            self.index.insert(msg.clone(), self.order.len());
            self.order.push((msg, n));
        }
    }

    /// Appends another set (keeps first-seen order: ours, then theirs).
    pub fn merge(&mut self, other: Warnings) {
        for (m, n) in other.order {
            self.add_n(m, n);
        }
    }

    /// `"message"` once, `"message (×n)"` when repeated.
    pub fn into_strings(self) -> Vec<String> {
        self.order
            .into_iter()
            .map(|(m, n)| if n == 1 { m } else { format!("{m} (×{n})") })
            .collect()
    }
}
