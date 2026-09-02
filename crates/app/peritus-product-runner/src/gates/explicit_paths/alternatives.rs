//! Recognition of explicit output paths joined by alternative cardinality.

use std::path::PathBuf;

use super::normalized;

pub(super) fn groups(words: &[&str], mentions: &[(usize, PathBuf, bool)]) -> Vec<Vec<PathBuf>> {
    let required = mentions.iter().filter(|(_, _, required)| *required).collect::<Vec<_>>();
    if required.len() < 2 {
        return Vec::new();
    }
    let has_or = required.windows(2).any(|pair| separated_by_or(words, pair));
    if has_or && alternative_cardinality(&words[..required[0].0]) {
        return vec![required.iter().map(|(_, path, _)| (*path).clone()).collect()];
    }

    let mut groups = Vec::new();
    let mut current = Vec::new();
    for pair in required.windows(2) {
        if separated_by_or(words, pair) {
            if current.is_empty() {
                current.push(pair[0].1.clone());
            }
            current.push(pair[1].1.clone());
        } else if !current.is_empty() {
            groups.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn separated_by_or(words: &[&str], pair: &[&(usize, PathBuf, bool)]) -> bool {
    words[pair[0].0.saturating_add(1)..pair[1].0].iter().any(|word| normalized(word) == "or")
}

fn alternative_cardinality(words: &[&str]) -> bool {
    let normalized = words.iter().map(|word| normalized(word)).collect::<Vec<_>>();
    normalized.windows(2).any(|pair| pair == ["one", "of"])
        || normalized.windows(3).any(|triple| triple == ["at", "least", "one"])
}
