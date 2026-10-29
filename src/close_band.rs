//! The "Press Esc again to close" band: a timed overlay on the control-hint rows.
//!
//! Terminals have no alpha, so opacity is faked by interpolating each cell's RGB
//! between the underlying colour and the target. The band background fades over
//! the whole fade-in and fade-out; stars and text are staggered inside those
//! fades so that they never transition at the same time.
use crate::render::{Canvas, Cell};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

// ---- Tunables (not user settings): edit here to adjust the animation. ----
/// Quick fade-in of the band background.
pub const FADE_IN_MS: u64 = 112;
/// How long the band stays fully opaque.
pub const HOLD_MS: u64 = 1000;
/// Slow fade-out of the band background.
pub const FADE_OUT_MS: u64 = 1000;
/// Share (permille) of each fade given to the stars. Fade-in: stars fade out over
/// the first 25%, text fades in over the remaining 75%. Fade-out is the reverse:
/// text fades out over the first 75%, stars return over the last 25%.
pub const STAR_SPLIT_PERMILLE: u64 = 250;
/// A second Esc still closes until this share (permille) of the fade-out has
/// passed (0.75); afterwards Esc counts as a first press again.
pub const CLOSE_CUTOFF_PERMILLE: u64 = 750;
/// Easing applied to every opacity ramp (input and output in permille, 0..=1000).
/// Linear by default; swap in e.g. a smoothstep to change the feel.
pub fn ease(p: i64) -> i64 {
    p
}
// ---------------------------------------------------------------------------

pub const FADE_IN: Duration = Duration::from_millis(FADE_IN_MS);
pub const HOLD: Duration = Duration::from_millis(HOLD_MS);
pub const FADE_OUT: Duration = Duration::from_millis(FADE_OUT_MS);
/// Total animation length.
pub const TOTAL: Duration = Duration::from_millis(FADE_IN_MS + HOLD_MS + FADE_OUT_MS);
/// Time after the first Esc until which a second Esc closes.
pub const CLOSE_WINDOW: Duration =
    Duration::from_millis(FADE_IN_MS + HOLD_MS + FADE_OUT_MS * CLOSE_CUTOFF_PERMILLE / 1000);
pub const MESSAGE: &str = "Press Esc again to close";

/// Opacity of each layer in permille.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase {
    /// Band background over the underlying cells.
    pub background: i64,
    /// How far the underlying stars (and hints) have faded towards the band.
    pub stars_hidden: i64,
    /// Message text over the band.
    pub text: i64,
}

fn ramp(elapsed: Duration, start: Duration, length: Duration) -> i64 {
    let linear = if elapsed <= start {
        0
    } else if elapsed >= start + length {
        1000
    } else {
        ((elapsed - start).as_micros() * 1000 / length.as_micros()) as i64
    };
    ease(linear)
}

/// Layer opacities `elapsed` after the first Esc; `None` once finished.
pub fn phase(elapsed: Duration) -> Option<Phase> {
    if elapsed >= TOTAL {
        return None;
    }
    let in_star = FADE_IN * STAR_SPLIT_PERMILLE as u32 / 1000;
    let out_start = FADE_IN + HOLD;
    let out_text = FADE_OUT * (1000 - STAR_SPLIT_PERMILLE) as u32 / 1000;
    Some(Phase {
        background: ramp(elapsed, Duration::ZERO, FADE_IN) - ramp(elapsed, out_start, FADE_OUT),
        stars_hidden: ramp(elapsed, Duration::ZERO, in_star)
            - ramp(elapsed, out_start + out_text, FADE_OUT - out_text),
        text: ramp(elapsed, in_star, FADE_IN - in_star) - ramp(elapsed, out_start, out_text),
    })
}

fn mix(a: [u8; 3], b: [u8; 3], p: i64) -> [u8; 3] {
    std::array::from_fn(|i| {
        ((a[i] as i64 * (1000 - p) + b[i] as i64 * p) / 1000).clamp(0, 255) as u8
    })
}

/// Overlay the bottom `rows` rows. `base` is the terminal background beneath
/// cells without one, `band` the topbar colour and `text` the message colour.
pub fn paint(
    canvas: &mut Canvas,
    rows: usize,
    phase: Phase,
    base: [u8; 3],
    band: [u8; 3],
    text: [u8; 3],
) {
    let (w, h) = (canvas.width, canvas.height);
    let rows = rows.min(h);
    for y in h - rows..h {
        for x in 0..w {
            let cell = &mut canvas.cells[y * w + x];
            let under = cell.bg.unwrap_or(base);
            let bg = mix(under, band, phase.background);
            // Underlying glyphs fade towards what is now the cell's background.
            cell.fg = mix(cell.fg, bg, phase.stars_hidden);
            if phase.stars_hidden >= 1000 {
                cell.glyph = " ".into();
                cell.bold = false;
                cell.strike = false;
            }
            cell.bg = Some(bg);
        }
    }
    if phase.text <= 0 || rows == 0 {
        return;
    }
    let y = h - rows + (rows - 1) / 2;
    let x = w.saturating_sub(UnicodeWidthStr::width(MESSAGE)) / 2;
    let bg = mix(base, band, phase.background);
    canvas.text(
        x,
        y,
        MESSAGE,
        Cell {
            fg: mix(bg, text, phase.text),
            bg: Some(bg),
            bold: true,
            ..Cell::default()
        },
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(d: Duration) -> Phase {
        phase(d).unwrap()
    }
    fn frac(d: Duration, permille: u64) -> Duration {
        d * permille as u32 / 1000
    }
    #[test]
    fn layers_are_staggered_by_the_split_and_never_all_transition_at_once() {
        let star_in = frac(FADE_IN, STAR_SPLIT_PERMILLE);
        let out = FADE_IN + HOLD;
        let text_out = frac(FADE_OUT, 1000 - STAR_SPLIT_PERMILLE);
        let eps = Duration::from_millis(1);
        assert_eq!(
            at(Duration::ZERO),
            Phase {
                background: 0,
                stars_hidden: 0,
                text: 0
            }
        );
        // Fade-in: stars first, text only afterwards; background across all of it.
        let p = at(star_in / 2);
        assert!(p.stars_hidden > 0 && p.stars_hidden < 1000 && p.text == 0 && p.background > 0);
        let p = at(star_in);
        assert_eq!((p.stars_hidden, p.text), (1000, 0));
        let p = at(star_in + (FADE_IN - star_in) / 2);
        assert!(p.stars_hidden == 1000 && p.text > 0 && p.text < 1000 && p.background < 1000);
        assert_eq!(
            at(FADE_IN),
            Phase {
                background: 1000,
                stars_hidden: 1000,
                text: 1000
            }
        );
        assert_eq!(
            at(out - eps),
            Phase {
                background: 1000,
                stars_hidden: 1000,
                text: 1000
            }
        );
        // Fade-out: text first, stars only afterwards; background across all of it.
        let p = at(out + text_out / 2);
        assert!(p.text > 0 && p.text < 1000 && p.stars_hidden == 1000 && p.background < 1000);
        let p = at(out + text_out);
        assert_eq!((p.text, p.stars_hidden), (0, 1000));
        let p = at(out + text_out + (FADE_OUT - text_out) / 2);
        assert!(p.text == 0 && p.stars_hidden > 0 && p.stars_hidden < 1000 && p.background > 0);
        let end = at(TOTAL - eps);
        assert!(end.text == 0 && end.stars_hidden < 20 && end.background < 20);
        assert!(phase(TOTAL).is_none());
    }
    #[test]
    fn close_window_ends_at_the_cutoff_of_the_fade_out() {
        assert_eq!(
            CLOSE_WINDOW,
            FADE_IN + HOLD + frac(FADE_OUT, CLOSE_CUTOFF_PERMILLE)
        );
        assert!(CLOSE_WINDOW > FADE_IN + HOLD && CLOSE_WINDOW < TOTAL);
    }
    #[test]
    fn paint_interpolates_cells_and_centres_bold_text() {
        let mut c = Canvas::new(40, 4);
        for cell in &mut c.cells {
            cell.glyph = "*".into();
            cell.fg = [200; 3];
        }
        let full = Phase {
            background: 1000,
            stars_hidden: 1000,
            text: 1000,
        };
        paint(&mut c, 2, full, [0; 3], [100, 50, 10], [255; 3]);
        assert_eq!(c.cells[0].glyph, "*", "rows above are untouched");
        let row: String = (0..40).map(|x| c.cells[3 * 40 + x].glyph.clone()).collect();
        assert_eq!(row, " ".repeat(40));
        let row: String = (0..40).map(|x| c.cells[2 * 40 + x].glyph.clone()).collect();
        assert_eq!(row.trim(), MESSAGE);
        let at = row.find('P').unwrap();
        assert_eq!(at, (40 - MESSAGE.len()) / 2);
        let t = &c.cells[2 * 40 + at];
        assert!(t.bold && t.fg == [255; 3] && t.bg == Some([100, 50, 10]));
        let mut c = Canvas::new(40, 2);
        let half = Phase {
            background: 500,
            stars_hidden: 0,
            text: 0,
        };
        paint(&mut c, 1, half, [0; 3], [100, 50, 10], [255; 3]);
        assert_eq!(c.cells[40].bg, Some([50, 25, 5]));
        assert_eq!(c.cells[40].fg, Cell::default().fg);
    }
}
