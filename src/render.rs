//! Cell-based renderer: the star field is terminal geometry, never list geometry.
use std::fmt::Write;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: String,
    pub fg: [u8; 3],
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
    pub strike: bool,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: " ".into(),
            fg: [188; 3],
            bg: None,
            bold: false,
            strike: false,
        }
    }
}
#[derive(Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    pub theme: Option<Theme>,
    transient: Vec<bool>,
    baseline_blocked: Vec<bool>,
    foreground: Vec<bool>,
}
impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            width: w,
            height: h,
            cells: vec![Cell::default(); w * h],
            theme: None,
            transient: vec![false; w * h],
            baseline_blocked: vec![false; w * h],
            foreground: vec![false; w * h],
        }
    }
    pub fn text(&mut self, x: usize, y: usize, text: &str, style: Cell, transparent: bool) {
        if y >= self.height {
            return;
        }
        let mut x = x;
        for glyph in text.graphemes(true) {
            if glyph.chars().any(char::is_control) {
                continue;
            }
            let width = UnicodeWidthStr::width(glyph);
            if width == 0 {
                continue;
            }
            if x + width > self.width {
                break;
            }
            if transparent && glyph == " " {
                self.baseline_blocked[y * self.width + x] = true;
                if !self.transient[y * self.width + x] {
                    self.cells[y * self.width + x] = Cell::default();
                }
            } else {
                self.foreground[y * self.width + x] = true;
                self.cells[y * self.width + x] = Cell {
                    glyph: glyph.into(),
                    ..style.clone()
                };
                for offset in 1..width {
                    self.foreground[y * self.width + x + offset] = true;
                    self.cells[y * self.width + x + offset] = Cell {
                        glyph: String::new(),
                        ..style.clone()
                    };
                }
            }
            x += width;
        }
    }
    pub fn accent_text(&mut self, x: usize, y: usize, text: &str, accent: Accent) {
        let mut x = x;
        for glyph in text.graphemes(true) {
            let cell = self
                .theme
                .as_ref()
                .map(|t| t.accent(x, y, &accent))
                .unwrap_or_else(|| Cell {
                    fg: [215, 135, 255],
                    bold: accent.bold,
                    strike: accent.strike,
                    ..Cell::default()
                });
            // Hints retain transparent whitespace, including baseline stars.
            if glyph != " " {
                self.text(x, y, glyph, cell, false);
            }
            x += UnicodeWidthStr::width(glyph);
        }
    }
    pub fn diff(&self, old: Option<&Self>) -> String {
        let old = old.filter(|o| o.width == self.width && o.height == self.height);
        let mut out = String::new();
        let mut last_style = None;
        let mut previous: Option<(usize, usize)> = None;
        for (i, c) in self.cells.iter().enumerate() {
            if c.glyph.is_empty() || old.is_some_and(|o| o.cells[i] == *c) {
                continue;
            }
            let (x, y) = (i % self.width, i / self.width);
            if previous != Some((x, y)) {
                let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            }
            let style = (&c.fg, &c.bg, c.bold, c.strike);
            if last_style != Some(style) {
                let _ = write!(
                    out,
                    "\x1b[0;{};{};38;2;{};{};{}m",
                    if c.bold { 1 } else { 22 },
                    if c.strike { 9 } else { 29 },
                    c.fg[0],
                    c.fg[1],
                    c.fg[2]
                );
                if let Some(bg) = c.bg {
                    let _ = write!(out, "\x1b[48;2;{};{};{}m", bg[0], bg[1], bg[2]);
                }
                last_style = Some(style);
            }
            out.push_str(&c.glyph);
            previous = Some((x + UnicodeWidthStr::width(c.glyph.as_str()), y));
        }
        out
    }
}

#[derive(Clone, Copy)]
pub struct Accent {
    pub soft: bool,
    pub brightness: i64,
    pub flash: bool,
    pub bold: bool,
    pub strike: bool,
}
impl Default for Accent {
    fn default() -> Self {
        Self {
            soft: false,
            brightness: 1000,
            flash: true,
            bold: false,
            strike: false,
        }
    }
}
#[derive(Clone)]
pub struct Theme {
    pub now: u64,
    previous: i64,
    next: i64,
    width: usize,
    height: usize,
}
impl Theme {
    fn local(&self, x: usize, y: usize) -> i64 {
        let distance = (x.min(self.width.saturating_sub(1)) * 1000
            / self.width.saturating_sub(1).max(1)
            + y.saturating_sub(2).min(self.height.saturating_sub(3)) * 1000
                / self.height.saturating_sub(3).max(1))
            / 2;
        let (mut lo, mut hi) = (0, 1000);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if shoulder(mid) < distance as i64 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (self.now % 4000) as i64 - 1167 * lo / 1000
    }
    fn accent(&self, x: usize, y: usize, style: &Accent) -> Cell {
        let local = self.local(x, y);
        let active = self.now >= 4000;
        let white = if !active || !(0..300).contains(&local) {
            0
        } else if local < 60 {
            smooth(local * 1000 / 60)
        } else {
            smooth(1000 - (local - 60) * 1000 / 240)
        };
        let hue = if active && local < 60 {
            self.previous
        } else {
            self.next
        };
        Cell {
            fg: dim(
                mix(
                    hsv(80 + hue, if style.soft { 350 } else { 700 }, 255),
                    [255; 3],
                    white,
                ),
                style.brightness,
            ),
            bold: style.bold || (style.flash && white >= 900),
            strike: style.strike,
            ..Cell::default()
        }
    }
    pub fn bar(&self, secondary: bool) -> [u8; 3] {
        let p = if self.now < 4000 {
            1000
        } else {
            shoulder((self.now % 4000) as i64 * 1000 / 1467)
        };
        let center = if secondary { 260 } else { 80 };
        dim(
            mix(
                hsv(center + self.previous, 350, 255),
                hsv(center + self.next, 350, 255),
                p,
            ),
            550,
        )
    }
}
pub fn smooth(x: i64) -> i64 {
    let x = x.clamp(0, 1000);
    x * x * (3000 - 2 * x) / 1_000_000
}
pub fn shoulder(p: i64) -> i64 {
    let p = p.clamp(0, 1000);
    let reflect = p > 500;
    let p = if reflect { 1000 - p } else { p };
    let q = if p <= 400 {
        p * p * 25
    } else {
        let u = p - 400;
        4_000_000 + 2 * (u * 10000 + u * u * u)
    };
    if reflect {
        (16_000_000 - q) / 16000
    } else {
        q / 16000
    }
}
pub fn hsv(h: i64, s: i64, v: i64) -> [u8; 3] {
    let h = h.rem_euclid(360);
    let c = v * s / 1000;
    let x = c * (60 - (h % 120 - 60).abs()) / 60;
    let m = v - c;
    let rgb = match h / 60 {
        0 => [c, x, 0],
        1 => [x, c, 0],
        2 => [0, c, x],
        3 => [0, x, c],
        4 => [x, 0, c],
        _ => [c, 0, x],
    };
    rgb.map(|v| (v + m).clamp(0, 255) as u8)
}
fn mix(a: [u8; 3], b: [u8; 3], p: i64) -> [u8; 3] {
    std::array::from_fn(|i| {
        ((a[i] as i64 * (1000 - p) + b[i] as i64 * p) / 1000).clamp(0, 255) as u8
    })
}
fn dim(c: [u8; 3], p: i64) -> [u8; 3] {
    c.map(|v| (v as i64 * p / 1000).clamp(0, 255) as u8)
}
#[derive(Clone)]
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 34
    }
    fn range(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
#[derive(Clone)]
struct Star {
    glyph: char,
    sat: i64,
    offset: i64,
    value: i64,
    birth: Option<u64>,
}
#[derive(Clone)]
struct Twinkle {
    index: usize,
    start: u64,
    star: Star,
    consumed: bool,
}
pub struct Field {
    w: usize,
    h: usize,
    seed: u64,
    rng: Rng,
    stars: Vec<Option<Star>>,
    twinkles: Vec<Twinkle>,
    debts: Vec<usize>,
    next_birth: u64,
    next_horizon: u64,
    last_tick: Option<u64>,
    arrival: Vec<i64>,
    hue_cycle: u64,
    hue: i64,
    previous_hue: i64,
    hue_rng: u32,
    baseline_blocked: Vec<bool>,
    foreground: Vec<bool>,
    pub options_top: usize,
}
impl Field {
    pub fn new(w: usize, h: usize, seed: u64) -> Self {
        let mut field = Self {
            w,
            h,
            seed,
            rng: Rng(seed),
            stars: vec![None; w * h],
            twinkles: vec![],
            debts: vec![],
            next_birth: 1,
            next_horizon: 1,
            last_tick: None,
            arrival: vec![0; 1001],
            hue_cycle: 0,
            hue: 0,
            previous_hue: 0,
            hue_rng: seed as u32,
            baseline_blocked: vec![false; w * h],
            foreground: vec![false; w * h],
            options_top: 3,
        };
        let mut distance = 0;
        for p in 0..=1000 {
            while distance <= shoulder(p) as usize && distance <= 1000 {
                field.arrival[distance] = 1167 * p / 1000;
                distance += 1;
            }
        }
        for y in 2..h {
            for x in 0..w {
                if field.rng.range(800000) < (field.density(y) * 50) as u64 {
                    let star = field.star(None);
                    field.stars[y * w + x] = Some(star);
                }
            }
        }
        field
    }
    fn density(&self, y: usize) -> i64 {
        let depth = (y.saturating_sub(2) * 1000 / self.h.saturating_sub(3).max(1)) as i64;
        1000 + 5000 * depth * depth / 1_000_000
    }
    fn star(&mut self, birth: Option<u64>) -> Star {
        let palette = self.rng.range(3) as usize;
        let min = [228, 187, 228][palette];
        let sat = min + self.rng.range((801 - min) as u64) as i64;
        let offset = loop {
            let sum = (0..12).map(|_| self.rng.range(1000) as i64).sum::<i64>() - 5994;
            if (-3000..=3000).contains(&sum) {
                break sum * 60 / 6000;
            }
        };
        Star {
            glyph: ['*', '.', '+'][self.rng.range(3) as usize],
            sat,
            offset,
            value: [175, 215, 175][palette],
            birth,
        }
    }
    pub fn resize(&mut self, w: usize, h: usize) {
        if (w, h) != (self.w, self.h) {
            *self = Self::new(w, h, self.seed);
        }
    }
    pub fn apply_masks(&mut self, canvas: &Canvas) {
        self.baseline_blocked.clone_from(&canvas.baseline_blocked);
        self.foreground.clone_from(&canvas.foreground);
        for (i, blocked) in self.baseline_blocked.iter().enumerate() {
            if *blocked && self.stars[i].take().is_some() {
                self.debts.push(i);
            }
        }
    }
    fn rotate(&mut self, cycle: u64) -> i64 {
        if cycle + 1 == self.hue_cycle {
            return self.previous_hue;
        }
        if cycle < self.hue_cycle {
            self.hue_cycle = 0;
            self.hue = 0;
            self.hue_rng = self.seed as u32;
        }
        while self.hue_cycle < cycle {
            loop {
                self.hue_rng = self.hue_rng.wrapping_mul(1664525).wrapping_add(1013904223);
                if self.hue_rng < 4294967239 {
                    break;
                }
            }
            self.previous_hue = self.hue;
            self.hue = (self.hue + 60 + (self.hue_rng % 61) as i64) % 360;
            self.hue_cycle += 1;
        }
        self.hue
    }
    pub fn accent(&mut self, now: u64) -> [u8; 3] {
        let cycle = now / 4000;
        let prev = self.rotate(cycle.saturating_sub(1));
        let next = self.rotate(cycle);
        let p = if cycle == 0 {
            1000
        } else {
            shoulder((now % 4000) as i64 * 1000 / 1467)
        };
        mix(hsv(80 + prev, 350, 255), hsv(80 + next, 350, 255), p)
    }
    fn advance(&mut self, now: u64) {
        if self.last_tick == Some(now / 50) {
            return;
        }
        self.last_tick = Some(now / 50);
        self.twinkles.retain(|t| {
            if now.saturating_sub(t.start) >= 900 {
                if t.consumed {
                    self.debts.push(t.index);
                }
                false
            } else {
                true
            }
        });
        let phase = ((now as i64 - 1467 / 2 + 500 + 4000) % 4000) * 1000 / 4000;
        let product = phase * (1000 - phase);
        let sine = 16000 * product / (5_000_000 - 4 * product);
        let weight = 200 + 800 * sine * sine / 1_000_000;
        if self.w > 0 && self.h > 2 {
            let baseline = now >= self.next_birth;
            let horizon = now >= self.next_horizon;
            if baseline {
                self.next_birth = now + 1 + self.rng.range(200);
            }
            if horizon {
                let excess = (2..self.h)
                    .map(|y| (self.density(y) - 1000) as u64)
                    .sum::<u64>()
                    * self.w as u64;
                self.next_horizon = now
                    + (50_000_000 * ((self.h - 2) * self.w) as u64 / (250 * excess.max(1))).max(1);
            }
            if (baseline || horizon) && self.rng.range(1000) < weight as u64 {
                let start = 2 * self.w + self.rng.range(((self.h - 2) * self.w) as u64) as usize;
                for n in 0..(self.h - 2) * self.w {
                    let index = 2 * self.w + (start - 2 * self.w + n) % ((self.h - 2) * self.w);
                    if self.foreground[index] || self.twinkles.iter().any(|t| t.index == index) {
                        continue;
                    }
                    if !baseline
                        && self.rng.range(5000) >= (self.density(index / self.w) - 1000) as u64
                    {
                        continue;
                    }
                    let consumed = self.stars[index].is_some();
                    let star = self.stars[index].take().unwrap_or_else(|| self.star(None));
                    self.twinkles.push(Twinkle {
                        index,
                        start: now,
                        star,
                        consumed,
                    });
                    break;
                }
            }
            for _ in 0..32 {
                let Some(origin) = self.debts.last().copied() else {
                    break;
                };
                let index = 2 * self.w + self.rng.range(((self.h - 2) * self.w) as u64) as usize;
                if index == origin
                    || self.baseline_blocked[index]
                    || self.stars[index].is_some()
                    || self.twinkles.iter().any(|t| t.index == index)
                    || self.rng.range(6000) >= self.density(index / self.w) as u64
                {
                    continue;
                }
                let star = self.star(Some(now));
                self.stars[index] = Some(star);
                self.debts.pop();
            }
        }
    }
    pub fn paint(&mut self, now: u64) -> Canvas {
        self.advance(now);
        let cycle = now / 4000;
        let previous = self.rotate(cycle.saturating_sub(1));
        let next = self.rotate(cycle);
        let phase = (now % 4000) as i64;
        let mut canvas = Canvas::new(self.w, self.h);
        canvas.theme = Some(Theme {
            now,
            previous,
            next,
            width: self.w,
            height: self.h,
        });
        for index in 2 * self.w..self.w * self.h {
            let twinkle = self.twinkles.iter().find(|t| t.index == index);
            let Some(star) = twinkle.map(|t| &t.star).or(self.stars[index].as_ref()) else {
                continue;
            };
            let (x, y) = (index % self.w, index / self.w);
            let distance = (x * 1000 / self.w.saturating_sub(1).max(1)
                + (y - 2) * 1000 / self.h.saturating_sub(3).max(1))
                / 2;
            let local = phase - self.arrival[distance];
            let white = if cycle == 0 || !(0..300).contains(&local) {
                0
            } else if local < 60 {
                smooth(local * 1000 / 60)
            } else {
                smooth(1000 - (local - 60) * 1000 / 240)
            };
            let hue = if cycle > 0 && local < 60 {
                previous
            } else {
                next
            };
            let color = hsv(260 + hue + star.offset, star.sat, star.value);
            let age = twinkle.map(|t| now.saturating_sub(t.start) as i64);
            let intensity = age
                .map(|a| {
                    if a < 120 {
                        smooth(a * 1000 / 120)
                    } else {
                        smooth(1000 - (a - 120) * 1000 / 780)
                    }
                })
                .unwrap_or(1000);
            let twinkle_white = if twinkle.is_some() { intensity } else { 0 };
            let white_mix = 1000 - (1000 - white) * (1000 - twinkle_white) / 1000;
            let fade = if y < self.options_top {
                800
            } else {
                (65 - 60 * (y - self.options_top) as i64
                    / self.h.saturating_sub(self.options_top).max(1) as i64)
                    * 10
            };
            let peak = (10000 * (self.h - y) as i64 / (7 * self.h.saturating_sub(2).max(1)) as i64)
                .clamp(4, 1000);
            let brightness = if white > 0 {
                fade * (1000 - white) / 1000 + peak * white / 1000
            } else {
                fade
            };
            let replacement = star
                .birth
                .map(|b| smooth(now.saturating_sub(b) as i64 * 1000 / 500))
                .unwrap_or(1000);
            let fg = dim(
                mix(color, [255; 3], white_mix),
                brightness * intensity / 1000 * replacement / 1000,
            );
            let glyph = if twinkle.is_some() {
                if intensity < 250 {
                    '.'
                } else if intensity < 650 {
                    '+'
                } else {
                    '*'
                }
            } else {
                star.glyph
            };
            canvas.cells[index] = Cell {
                glyph: glyph.to_string(),
                fg,
                bold: white >= 900,
                ..Cell::default()
            };
            canvas.transient[index] = twinkle.is_some();
        }
        canvas
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audited_accent_palette_and_sweep_peak_are_shared_without_hint_bold() {
        let mut field = Field::new(80, 24, 1967);
        let canvas = field.paint(0);
        let theme = canvas.theme.unwrap();
        assert_eq!(theme.accent(0, 2, &Accent::default()).fg, [195, 255, 77]);
        assert_eq!(theme.bar(false), [123, 140, 91]);
        assert_eq!(theme.bar(true), [107, 91, 140]);
        let theme = field.paint(4060).theme.unwrap();
        let marker = theme.accent(0, 2, &Accent::default());
        assert_eq!(marker.fg, [255; 3]);
        assert!(marker.bold);
        let hint = theme.accent(
            0,
            2,
            &Accent {
                soft: true,
                flash: false,
                brightness: 550,
                ..Accent::default()
            },
        );
        assert_eq!(hint.fg, [140; 3]);
        assert!(!hint.bold);
    }
    #[test]
    fn label_spaces_exclude_baseline_but_allow_twinkles_and_disabled_strike() {
        let mut c = Canvas::new(3, 1);
        for cell in &mut c.cells {
            cell.glyph = "*".into();
        }
        c.transient[1] = true;
        c.text(0, 0, "  ", Cell::default(), true);
        assert_eq!(c.cells[0].glyph, " ");
        assert_eq!(c.cells[1].glyph, "*");
        c.text(
            2,
            0,
            " ",
            Cell {
                strike: true,
                ..Cell::default()
            },
            false,
        );
        assert!(c.cells[2].strike);
        assert_eq!(c.cells[2].glyph, " ");
    }
    #[test]
    fn reference_curve_boundaries() {
        assert_eq!(shoulder(400), 250);
        assert_eq!(shoulder(500), 500);
        assert_eq!(shoulder(600), 750);
        assert_eq!(smooth(0), 0);
        assert_eq!(smooth(1000), 1000);
        assert_eq!(hsv(0, 1000, 255), [255, 0, 0]);
    }
    #[test]
    fn deterministic_field_and_full_height() {
        let mut a = Field::new(80, 24, 1967);
        let mut b = Field::new(80, 24, 1967);
        for time in (0..12000).step_by(50) {
            assert_eq!(a.paint(time).cells, b.paint(time).cells);
        }
        assert!(
            a.paint(12000).cells[80 * 23..]
                .iter()
                .any(|c| c.glyph != " ")
        );
    }
    #[test]
    fn option_overlay_does_not_move_field() {
        let mut f = Field::new(80, 24, 3);
        let original = f.paint(100);
        let mut first = original.clone();
        first.text(4, 4, "one", Cell::default(), false);
        let mut scrolled = f.paint(100);
        scrolled.text(4, 4, "two", Cell::default(), false);
        for i in 0..80 * 24 {
            if !(324..327).contains(&i) {
                assert_eq!(first.cells[i], scrolled.cells[i]);
            }
        }
    }
    #[test]
    fn wide_graphemes_clip_without_splitting() {
        let mut c = Canvas::new(4, 1);
        c.text(0, 0, "界e\u{301}界", Cell::default(), false);
        assert_eq!(c.cells[0].glyph, "界");
        assert_eq!(c.cells[1].glyph, "");
        assert_eq!(c.cells[2].glyph, "e\u{301}");
        assert_eq!(c.cells[3].glyph, " ");
    }
}
