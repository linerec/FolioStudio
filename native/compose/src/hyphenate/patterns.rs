use std::collections::HashMap;

#[derive(Default)]
struct Node {
    children: HashMap<char, usize>,
    weights: Vec<u8>,
}
pub(super) struct Patterns {
    nodes: Vec<Node>,
    exceptions: HashMap<String, Vec<usize>>,
}

fn section<'a>(text: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
    text.split(name)
        .skip(1)
        .filter_map(|part| part.split_once('}'))
        .flat_map(|(body, _)| body.split_whitespace())
}

impl Patterns {
    pub fn load(tex: &str, extra: Option<&str>) -> Self {
        let text = tex
            .lines()
            .map(|line| line.split('%').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let mut patterns = Self {
            nodes: vec![Node::default()],
            exceptions: HashMap::new(),
        };
        for token in section(&text, r"\patterns{").chain(extra) {
            patterns.add(token);
        }
        for entry in section(&text, r"\hyphenation{") {
            let mut word = String::new();
            let mut points = Vec::new();
            let mut length = 0;
            for c in entry.chars() {
                if c == '-' {
                    points.push(length);
                } else {
                    word.push(c);
                    length += 1;
                }
            }
            points.retain(|&p| p > 0 && p < length);
            patterns.exceptions.insert(word, points);
        }
        patterns
    }

    fn add(&mut self, token: &str) {
        let mut node = 0;
        let mut weights = vec![0];
        for c in token.chars() {
            if let Some(n) = c.to_digit(10) {
                *weights.last_mut().unwrap() = n as u8;
            } else {
                let next = self.nodes.len();
                node = *self.nodes[node].children.entry(c).or_insert(next);
                if node == next {
                    self.nodes.push(Node::default());
                }
                weights.push(0);
            }
        }
        let stored = &mut self.nodes[node].weights;
        stored.resize(weights.len(), 0);
        for (slot, weight) in stored.iter_mut().zip(weights) {
            *slot = (*slot).max(weight);
        }
    }

    pub fn exception(&self, word: &str) -> Option<&[usize]> {
        self.exceptions.get(word).map(Vec::as_slice)
    }

    pub fn points(&self, word: &str) -> Vec<usize> {
        let letters: Vec<_> = ['.'].into_iter().chain(word.chars()).chain(['.']).collect();
        let mut scores = vec![0; letters.len() + 1];
        for start in 0..letters.len() {
            let mut node = 0;
            for c in &letters[start..] {
                let Some(&next) = self.nodes[node].children.get(c) else {
                    break;
                };
                node = next;
                for (score, weight) in scores[start..].iter_mut().zip(&self.nodes[node].weights) {
                    *score = (*score).max(*weight);
                }
            }
        }
        (1..letters.len() - 2)
            .filter(|&p| scores[p + 1] % 2 == 1)
            .collect()
    }
}
