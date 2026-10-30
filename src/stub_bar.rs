//! Usage stub bar: drawn in place of a usage bar when the usage itself is
//! unavailable (not reported, unknown, unsupported).
//!
//! Half-cell model per side, from the center outward: `heavy` heavy half-cells,
//! then light dash segments (strictly decreasing outward, the last one half a
//! cell) separated by blank gaps, then one blank half-cell. The left side mirrors
//! the right, and each terminal cell renders two adjacent half-cells with a
//! box-drawing glyph. The operator-approved odd widths 3–33 are ground truth
//! ([`APPROVED`]); even widths sit between their odd neighbours and widths past 33
//! keep growing by the same rules.

/// One side of a stub bar, center to edge, in half-cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StubShape {
    pub heavy: usize,
    /// Light dash segments, strictly decreasing outward; the last is 1.
    pub segs: Vec<usize>,
    /// Blank half-cells between consecutive segments (one fewer than `segs`).
    pub gaps: Vec<usize>,
}

/// Approved shapes: (width, heavy, segments, gaps). Widths 7–33 are the
/// operator's table; 3 and 5 are its small-width forms.
const APPROVED: &[(usize, usize, &[usize], &[usize])] = &[
    (3, 1, &[1], &[]),
    (5, 3, &[1], &[]),
    (7, 4, &[2], &[]),
    (9, 5, &[3], &[]),
    (11, 5, &[3, 1], &[1]),
    (13, 6, &[4, 1], &[1]),
    (15, 6, &[3, 2, 1], &[1, 1]),
    (17, 7, &[4, 2, 1], &[1, 1]),
    (19, 7, &[5, 3, 1], &[1, 1]),
    (21, 8, &[5, 4, 1], &[1, 1]),
    (23, 9, &[6, 4, 1], &[1, 1]),
    (25, 9, &[7, 5, 1], &[1, 1]),
    (27, 10, &[6, 4, 2, 1], &[1, 1, 1]),
    (29, 10, &[6, 5, 3, 1], &[1, 1, 1]),
    (31, 11, &[7, 4, 2, 1], &[1, 2, 2]),
    (33, 11, &[7, 5, 3, 1], &[1, 2, 2]),
];
/// Widest approved shape; wider bars grow from it.
const LAST: usize = 33;

fn approved(width: usize) -> Option<StubShape> {
    APPROVED
        .iter()
        .find(|(w, ..)| *w == width)
        .map(|(_, heavy, segs, gaps)| StubShape {
            heavy: *heavy,
            segs: segs.to_vec(),
            gaps: gaps.to_vec(),
        })
}

/// Target heavy share of the width: shrinks slightly from 7 to 29, then holds.
fn heavy_target(width: usize) -> f64 {
    0.42 - 0.07 * ((width as f64 - 7.) / 22.).min(1.)
}
/// Preferred number of breaks (gaps) at a width.
fn breaks_target(width: usize) -> usize {
    match width {
        0..11 => 0,
        11..15 => 1,
        15..27 => 2,
        _ => 3,
    }
}
/// Lower is better: the reference solver's balance of core share, break count
/// and dash proportions.
fn score(width: usize, shape: &StubShape) -> f64 {
    let segs = &shape.segs;
    let mut s = (shape.heavy as f64 / width as f64 - heavy_target(width)).abs() * 40.;
    s += breaks_target(width).abs_diff(shape.gaps.len()) as f64 * 3.;
    for i in 1..segs.len().saturating_sub(1) {
        s += (segs[i] as f64 / segs[i - 1] as f64 - 0.62).abs() * 3.;
    }
    s + (segs[0] as f64 - shape.heavy as f64 * 0.6).abs() * 0.3
}

/// An even width between two approved odd neighbours: the heavy core and the
/// breaks stay between theirs, dashes strictly decrease to a final half-cell,
/// and the best-scoring candidate wins (first found on ties).
fn between(width: usize) -> Option<StubShape> {
    let (lo, hi) = (approved(width - 1)?, approved(width + 1)?);
    let mut best: Option<(f64, StubShape)> = None;
    for gaps in [&lo.gaps, &hi.gaps] {
        for heavy in lo.heavy..=hi.heavy {
            let Some(room) = width.checked_sub(heavy + 1 + gaps.iter().sum::<usize>()) else {
                continue;
            };
            let mut found = Vec::new();
            dashes(
                gaps.len() + 1,
                room,
                usize::MAX,
                &mut Vec::new(),
                &mut found,
            );
            for segs in found {
                let shape = StubShape {
                    heavy,
                    segs,
                    gaps: gaps.clone(),
                };
                let s = score(width, &shape);
                if best.as_ref().is_none_or(|(b, _)| s < *b - 1e-9) {
                    best = Some((s, shape));
                }
            }
        }
    }
    best.map(|(_, shape)| shape)
}
/// Every strictly decreasing split of `left` half-cells into `count` dashes whose
/// last is 1 (inner dashes at least 2); a single dash may be any length.
fn dashes(
    count: usize,
    left: usize,
    below: usize,
    acc: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if count == 1 {
        let ok = if acc.is_empty() { left >= 1 } else { left == 1 };
        if ok && left < below {
            acc.push(left);
            out.push(acc.clone());
            acc.pop();
        }
        return;
    }
    for v in 2..=left.saturating_sub(1).min(below.saturating_sub(1)) {
        acc.push(v);
        dashes(count - 1, left - v, v, acc, out);
        acc.pop();
    }
}

/// Past the table: gaps stay `[1, 2, 2]` (three breaks), the core grows one
/// half-cell per three, and the rest goes to the inner dashes in roughly 1 : 0.62
/// : 0.38 proportion, never breaking the strict decrease. Grows one half-cell at a
/// time from 33, so nothing ever shrinks.
fn beyond(width: usize) -> StubShape {
    let mut shape = approved(LAST).expect("widest approved shape");
    const WEIGHTS: [f64; 3] = [1., 0.62, 0.3844];
    let total: f64 = WEIGHTS.iter().sum();
    for w in LAST + 1..=width {
        let heavy = 11 + (w - LAST) / 3;
        if heavy > shape.heavy {
            shape.heavy = heavy;
            continue;
        }
        let s = &mut shape.segs;
        let inner = (s[0] + s[1] + s[2] + 1) as f64;
        let mut pick = 0;
        let mut deficit = f64::MIN;
        for i in 0..3 {
            if i > 0 && s[i] + 1 >= s[i - 1] {
                continue;
            }
            let d = inner * WEIGHTS[i] / total - s[i] as f64;
            if d > deficit + 1e-9 {
                deficit = d;
                pick = i;
            }
        }
        s[pick] += 1;
    }
    shape
}

/// The shape of a stub bar `width` cells wide; None below 3 cells (no room for
/// a heavy core between two half-cell ends).
pub fn stub_shape(width: usize) -> Option<StubShape> {
    match width {
        0..3 => None,
        w if w > LAST => Some(beyond(w)),
        w if w % 2 == 1 => approved(w),
        w => between(w),
    }
}

/// Half-cell weights (0 blank, 1 light, 2 heavy) of one side, center to edge.
fn side(shape: &StubShape) -> Vec<u8> {
    let mut a = vec![2; shape.heavy];
    for (i, len) in shape.segs.iter().enumerate() {
        if i > 0 {
            a.extend(std::iter::repeat_n(0, shape.gaps[i - 1]));
        }
        a.extend(std::iter::repeat_n(1, *len));
    }
    a.push(0);
    a
}

fn glyph(left: u8, right: u8) -> &'static str {
    match (left, right) {
        (2, 2) => "━",
        (1, 1) => "─",
        (2, 1) => "╾",
        (1, 2) => "╼",
        (1, 0) => "╴",
        (0, 1) => "╶",
        (0, 0) => " ",
        (2, 0) => "╸",
        (0, 2) => "╺",
        _ => "─",
    }
}

/// The stub bar exactly `width` cells wide: symmetric, one cell per glyph.
pub fn stub_bar(width: usize) -> String {
    match width {
        0 => String::new(),
        1 => "─".into(),
        2 => "╶╴".into(),
        w => {
            let shape = stub_shape(w).expect("shape for widths from 3");
            let right = side(&shape);
            let half: Vec<u8> = right.iter().rev().chain(&right).copied().collect();
            debug_assert_eq!(half.len(), 2 * w);
            half.chunks(2).map(|p| glyph(p[0], p[1])).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    /// The operator-approved table, byte for byte.
    const TABLE: &[(usize, &str)] = &[
        (7, "╶╼━━━╾╴"),
        (9, "╶─━━━━━─╴"),
        (11, "╶╶─━━━━━─╴╴"),
        (13, "╶╶─╼━━━━━╾─╴╴"),
        (15, "╶╶╴─╼━━━━━╾─╶╴╴"),
        (17, "╶╶╴──━━━━━━━──╶╴╴"),
        (19, "╶╶─╶──━━━━━━━──╴─╴╴"),
        (21, "╶╶─╴──╼━━━━━━━╾──╶─╴╴"),
        (23, "╶╶─╴───━━━━━━━━━───╶─╴╴"),
        (25, "╶╶──╶───━━━━━━━━━───╴──╴╴"),
        (27, "╶╶╴──╶──╼━━━━━━━━━╾──╴──╶╴╴"),
        (29, "╶╶─╶──╶──╼━━━━━━━━━╾──╴──╴─╴╴"),
        (31, "╶ ─ ──╶───━━━━━━━━━━━───╴── ─ ╴"),
        (33, "╶ ─╴╶──╶───━━━━━━━━━━━───╴──╴╶─ ╴"),
        (5, "╶━━━╴"),
        (3, "╶━╴"),
        (1, "─"),
    ];

    #[test]
    fn approved_table_is_reproduced_byte_for_byte() {
        for (width, expected) in TABLE {
            assert_eq!(stub_bar(*width), *expected, "width {width}");
        }
    }

    fn mirror(glyph: char) -> char {
        match glyph {
            '╾' => '╼',
            '╼' => '╾',
            '╴' => '╶',
            '╶' => '╴',
            '╸' => '╺',
            '╺' => '╸',
            other => other,
        }
    }

    #[test]
    fn every_width_is_exact_symmetric_and_single_cell() {
        for width in 0..=240 {
            let bar = stub_bar(width);
            assert_eq!(bar.chars().count(), width, "width {width}");
            assert_eq!(UnicodeWidthStr::width(bar.as_str()), width, "width {width}");
            for c in bar.chars() {
                assert_eq!(
                    UnicodeWidthStr::width(c.to_string().as_str()),
                    1,
                    "{c:?} at {width}"
                );
                assert!("━─╾╼╴╶ ".contains(c), "{c:?} at {width}");
            }
            let mirrored: String = bar.chars().rev().map(mirror).collect();
            assert_eq!(mirrored, bar, "width {width}");
            if width >= 2 {
                assert!(
                    bar.starts_with('╶') && bar.ends_with('╴'),
                    "ends at {width}: {bar}"
                );
            }
            assert_eq!(stub_bar(width), bar, "deterministic at {width}");
        }
    }

    #[test]
    fn derived_widths_follow_the_rules() {
        let mut heavy = 0;
        for width in 3..=240 {
            let shape = stub_shape(width).unwrap();
            let per_side = shape.heavy
                + shape.segs.iter().sum::<usize>()
                + shape.gaps.iter().sum::<usize>()
                + 1;
            assert_eq!(per_side, width, "half-cells at {width}");
            assert!(shape.heavy >= heavy, "core never shrinks at {width}");
            heavy = shape.heavy;
            assert_eq!(shape.gaps.len() + 1, shape.segs.len());
            assert!(shape.gaps.len() <= 3, "at most three breaks at {width}");
            if width <= 25 {
                assert!(shape.gaps.len() <= 2, "{width}");
            }
            if let Some(first) = shape.gaps.first() {
                assert_eq!(*first, 1, "first break is a half-cell at {width}");
            }
            if width <= 29 {
                assert!(shape.gaps.iter().all(|g| *g == 1), "{width}");
            } else if shape.gaps.contains(&2) {
                assert_eq!(shape.gaps, [1, 2, 2], "full gaps only as a pair at {width}");
            }
            if shape.segs.len() > 1 {
                assert_eq!(*shape.segs.last().unwrap(), 1, "{width}");
                assert!(shape.segs.windows(2).all(|p| p[0] > p[1]), "{width}");
            }
        }
        // Beyond the table: [1, 2, 2] gaps and the core keeps growing.
        assert_eq!(stub_shape(60).unwrap().gaps, [1, 2, 2]);
        assert!(stub_shape(60).unwrap().heavy > stub_shape(33).unwrap().heavy);
    }
}
