//! "Did you mean ...?" suggestions for unknown names.

/// The edit distance (insertions, deletions, substitutions) between `a`
/// and `b`, counted in characters.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = prev[j] + usize::from(ca != *cb);
            cur[j + 1] = substitute.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The most edits a suggestion may be away from a name of `len` characters.
fn max_distance(len: usize) -> usize {
    match len {
        0..=4 => 1,
        5..=9 => 2,
        _ => 3,
    }
}

/// The candidate closest to `target`, if exactly one is close enough to be
/// an obvious match. Candidates equal to `target` are ignored.
pub(crate) fn closest<'a>(
    target: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = max_distance(target.chars().count());
    let mut best: Option<(usize, &'a str)> = None;
    let mut tied = false;
    for candidate in candidates {
        if candidate == target {
            continue;
        }
        let d = distance(target, candidate);
        if d > limit {
            continue;
        }
        match best {
            Some((best_d, best_c)) if d == best_d && candidate != best_c => tied = true,
            Some((best_d, _)) if d >= best_d => {}
            _ => {
                best = Some((d, candidate));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("abc", ""), 3);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("travel.mode", "travel.mood"), 2);
        assert_eq!(distance("time", "tme"), 1);
    }

    #[test]
    fn suggestions() {
        assert_eq!(closest("tme", ["time", "day"]), Some("time"));
        assert_eq!(
            closest("travel.mod", ["travel.mode", "travel.route"]),
            Some("travel.mode")
        );
        // Too far, or ambiguous: no suggestion.
        assert_eq!(closest("zzz", ["time"]), None);
        assert_eq!(closest("ab", ["ac", "ad"]), None);
        assert_eq!(closest("time", ["time"]), None);
        assert_eq!(closest("x", []), None);
    }
}
