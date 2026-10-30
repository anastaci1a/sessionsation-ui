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
    /// Native terminal cursor, in zero-based cells; no shape/color override.
    pub cursor: Option<(usize, usize)>,
    /// Semantic host colors scoped to popup composition; never animate these.
    pub popup_palette: Option<crate::dialog::Palette>,
    background: Option<[u8; 3]>,
    transient: Vec<bool>,
    baseline_blocked: Vec<bool>,
    foreground: Vec<bool>,
    /// Star-field cells not covered by any text; only these may be paced.
    ambient: Vec<bool>,
    /// Terminal background beneath every layer; covered popups fade toward it.
    pub base: [u8; 3],
    /// Cells inside a popup painted this frame, and those already faded under a newer one.
    popup: Vec<bool>,
    faded: Vec<bool>,
    /// Animation time (ms) for canvases without a theme, such as popup content
    /// painted on a local canvas; text marquees read it.
    pub clock: u64,
}
/// How far popups beneath the top one fade toward the terminal background (permille).
pub const UNFOCUSED_FADE: i64 = 300;
/// Period of the selected picker arrow's pulse, in milliseconds.
pub const ARROW_PULSE_MS: u64 = 1000;
/// The darker hueshift level of unselected elements (permille of the normal hue).
pub const DIM_LEVEL: i64 = 600;
/// The even darker level an unselected Glow spinner dips to: the dim level scaled
/// down by itself.
pub const GLOW_FLOOR_LEVEL: i64 = DIM_LEVEL * DIM_LEVEL / 1000;
/// Star-field cell updates written per frame. The periodic sweep changes most
/// stars at once (about 30 kB per frame at 180x30); a slow terminal link (SSH)
/// then backs up behind HERDR's client, which stops reading keys until the burst
/// drains. Deferred stars are written next frame with their current value, so
/// text and selection changes are never delayed and nothing is lost.
pub const AMBIENT_CELLS_PER_FRAME: usize = 200;
impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            width: w,
            height: h,
            cells: vec![Cell::default(); w * h],
            theme: None,
            cursor: None,
            popup_palette: None,
            background: None,
            transient: vec![false; w * h],
            baseline_blocked: vec![false; w * h],
            foreground: vec![false; w * h],
            ambient: vec![false; w * h],
            base: [0; 3],
            popup: vec![false; w * h],
            faded: vec![false; w * h],
            clock: 0,
        }
    }
    /// A popup opens at this rectangle: every popup beneath it fades once toward
    /// the terminal background, so only the top popup looks focused.
    pub fn open_popup(&mut self, x: usize, y: usize, width: usize, height: usize) {
        for (i, cell) in self.cells.iter_mut().enumerate() {
            if self.popup[i] && !self.faded[i] {
                cell.fg = mix(cell.fg, self.base, UNFOCUSED_FADE);
                cell.bg = cell.bg.map(|bg| mix(bg, self.base, UNFOCUSED_FADE));
                self.faded[i] = true;
            }
        }
        for row in y..(y + height).min(self.height) {
            for col in x..(x + width).min(self.width) {
                self.popup[row * self.width + col] = true;
                self.faded[row * self.width + col] = false;
            }
        }
    }
    /// Composite a temporary panel without feeding its occlusion into star lifetimes.
    pub fn overlay(&mut self, paint: impl FnOnce(&mut Self)) {
        let baseline = self.baseline_blocked.clone();
        let foreground = self.foreground.clone();
        self.cursor = None;
        paint(self);
        self.baseline_blocked = baseline;
        self.foreground = foreground;
    }
    pub fn text(&mut self, x: usize, y: usize, text: &str, style: Cell, _transparent: bool) {
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
            if glyph == " " {
                self.baseline_blocked[y * self.width + x] = true;
            }
            {
                self.foreground[y * self.width + x] = true;
                self.ambient[y * self.width + x] = false;
                self.cells[y * self.width + x] = Cell {
                    glyph: glyph.into(),
                    bg: style.bg.or(self.background),
                    ..style.clone()
                };
                for offset in 1..width {
                    self.foreground[y * self.width + x + offset] = true;
                    self.ambient[y * self.width + x + offset] = false;
                    self.cells[y * self.width + x + offset] = Cell {
                        glyph: String::new(),
                        bg: style.bg.or(self.background),
                        ..style.clone()
                    };
                }
            }
            x += width;
        }
    }
    /// Resolve generic widget roles before drawing; explicit host RGB colors stay literal.
    pub fn widget_text(&mut self, x: usize, y: usize, text: &str, style: Cell, transparent: bool) {
        let style = if let Some(p) = &self.popup_palette {
            Cell {
                fg: match style.fg {
                    crate::widgets::SELECTED => p.text,
                    crate::widgets::SECONDARY => p.secondary,
                    crate::widgets::MUTED => p.muted,
                    [215, 0, 0] => p.error,
                    other => other,
                },
                ..style
            }
        } else {
            style
        };
        self.text(x, y, text, style, transparent);
    }
    pub fn accent_text(&mut self, x: usize, y: usize, text: &str, accent: Accent) {
        let mut x = x;
        for glyph in text.graphemes(true) {
            let cell = if let Some(p) = &self.popup_palette {
                Cell {
                    fg: if accent.soft { p.text } else { p.accent },
                    bold: accent.bold,
                    strike: accent.strike,
                    ..Cell::default()
                }
            } else if let Some(theme) = &self.theme {
                theme.accent(x, y, &accent)
            } else {
                Cell {
                    fg: [215, 135, 255],
                    bold: accent.bold,
                    strike: accent.strike,
                    ..Cell::default()
                }
            };
            // Spaces belong to the label: neither baseline stars nor twinkles may occupy them.
            self.text(x, y, glyph, cell, false);
            x += UnicodeWidthStr::width(glyph);
        }
    }
    pub fn diff(&self, old: Option<&Self>) -> String {
        self.encode(old, |_| true)
    }
    /// Like [`Canvas::diff`], but writes at most `limit` star-only cell changes,
    /// round-robin from `rotation`. Returns the delta and the canvas the terminal
    /// shows afterwards, which is the baseline for the next frame.
    pub fn paced_diff(
        &self,
        shown: Option<&Self>,
        limit: usize,
        rotation: &mut usize,
    ) -> (String, Self) {
        let Some(old) = shown.filter(|o| o.width == self.width && o.height == self.height) else {
            return (self.diff(None), self.clone());
        };
        let deferrable = (0..self.cells.len())
            .filter(|&i| {
                self.ambient[i]
                    && old.ambient[i]
                    && old.cells[i] != self.cells[i]
                    && old.cells[i].bg == self.cells[i].bg
            })
            .collect::<Vec<_>>();
        let mut deferred = vec![false; self.cells.len()];
        if deferrable.len() > limit {
            let start = deferrable.partition_point(|&i| i < *rotation);
            for k in limit..deferrable.len() {
                deferred[deferrable[(start + k) % deferrable.len()]] = true;
            }
            *rotation = deferrable[(start + limit) % deferrable.len()];
        }
        let mut next = self.clone();
        for (i, _) in deferred.iter().enumerate().filter(|(_, d)| **d) {
            next.cells[i] = old.cells[i].clone();
        }
        (self.encode(Some(old), |i| !deferred[i]), next)
    }
    fn encode(&self, old: Option<&Self>, write: impl Fn(usize) -> bool) -> String {
        let old = old.filter(|o| o.width == self.width && o.height == self.height);
        let mut out = String::new();
        let cursor = self
            .cursor
            .filter(|(x, y)| *x < self.width && *y < self.height);
        let old_cursor = old.and_then(|o| o.cursor);
        if cursor.is_none() && (old.is_none() || old_cursor.is_some()) {
            out.push_str("\x1b[?25l");
        }
        let mut last_style = None;
        let mut previous: Option<(usize, usize)> = None;
        for (i, c) in self.cells.iter().enumerate() {
            if c.glyph.is_empty() || old.is_some_and(|o| o.cells[i] == *c) || !write(i) {
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
        if let Some((x, y)) = cursor {
            if !out.is_empty() || cursor != old_cursor {
                let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            }
            if old_cursor.is_none() {
                out.push_str("\x1b[?25h");
            }
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
    pub(crate) fn accent(&self, x: usize, y: usize, style: &Accent) -> Cell {
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
    /// Grabbed picker arrow: a sinusoidal pulse between the selected-bullet
    /// accent and its dim level at (x, y), once per [`ARROW_PULSE_MS`].
    pub fn accent_pulse(&self, x: usize, y: usize) -> [u8; 3] {
        self.hue_pulse(x, y, 1000, DIM_LEVEL)
    }
    /// A sinusoidal pulse, once per [`ARROW_PULSE_MS`], from the hueshift at
    /// `high` (phase 0) to the one at `low` (mid-period), both sampled at (x, y).
    pub fn hue_pulse(&self, x: usize, y: usize, high: i64, low: i64) -> [u8; 3] {
        let level = |brightness| {
            self.accent(
                x,
                y,
                &Accent {
                    brightness,
                    ..Accent::default()
                },
            )
            .fg
        };
        mix(level(high), level(low), self.pulse())
    }
    fn pulse(&self) -> i64 {
        let phase = (self.now % ARROW_PULSE_MS) as f64 / ARROW_PULSE_MS as f64;
        ((1.0 - (phase * std::f64::consts::TAU).cos()) / 2.0 * 1000.0).round() as i64
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
    pub background: Option<[u8; 3]>,
    /// Star density and twinkle rate in per mille (0 = no stars).
    density_scale: i64,
    /// Star brightness in per mille.
    pub brightness_scale: i64,
}
impl Field {
    pub fn new(w: usize, h: usize, seed: u64) -> Self {
        Self::with_density(w, h, seed, 1000)
    }
    fn with_density(w: usize, h: usize, seed: u64, density: i64) -> Self {
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
            background: None,
            density_scale: density,
            brightness_scale: 1000,
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
        (1000 + 5000 * depth * depth / 1_000_000) * self.density_scale / 1000
    }
    /// Appearance settings: density changes regenerate the field; brightness is live.
    pub fn set_style(&mut self, density: i64, brightness: i64) {
        self.brightness_scale = brightness;
        if density != self.density_scale {
            let (w, h, seed) = (self.w, self.h, self.seed);
            let background = self.background;
            let top = self.options_top;
            *self = Self::with_density(w, h, seed, density);
            self.brightness_scale = brightness;
            self.background = background;
            self.options_top = top;
        }
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
            let brightness = self.brightness_scale;
            *self = Self::with_density(w, h, self.seed, self.density_scale);
            self.brightness_scale = brightness;
        }
    }
    /// Record this frame's text cells. Text only covers stars on the canvas;
    /// the stars themselves persist and show again once the text is gone.
    pub fn apply_masks(&mut self, canvas: &Canvas) {
        self.baseline_blocked.clone_from(&canvas.baseline_blocked);
        self.foreground.clone_from(&canvas.foreground);
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
                    .map(|y| (self.density(y) - self.density_scale).max(0) as u64)
                    .sum::<u64>()
                    * self.w as u64;
                self.next_horizon = now
                    + (50_000_000 * ((self.h - 2) * self.w) as u64 / (250 * excess.max(1))).max(1);
            }
            if (baseline || horizon)
                && self.density_scale > 0
                && self.rng.range(1000) < (weight * self.density_scale / 1000) as u64
            {
                let start = 2 * self.w + self.rng.range(((self.h - 2) * self.w) as u64) as usize;
                for n in 0..(self.h - 2) * self.w {
                    let index = 2 * self.w + (start - 2 * self.w + n) % ((self.h - 2) * self.w);
                    if self.foreground[index] || self.twinkles.iter().any(|t| t.index == index) {
                        continue;
                    }
                    if !baseline
                        && self.rng.range(5000)
                            >= (self.density(index / self.w) - self.density_scale).max(0) as u64
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
        canvas.base = self.background.unwrap_or([0; 3]);

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
            let fade = (fade * self.brightness_scale / 1000).min(1000);
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
            let background = self.background.unwrap_or([0; 3]);
            let mut target = mix(color, [255; 3], white_mix);
            // A star adds light: saturated hues must not dip below a colored
            // terminal background in any channel during gradient/birth/death fades.
            for (channel, base) in target.iter_mut().zip(background) {
                *channel = (*channel).max(base);
            }
            let fg = mix(
                background,
                target,
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
            canvas.ambient[index] = true;
        }
        canvas
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Live trace: every 4 s the star sweep rewrote ~770 stars per frame (about
    /// 24 kB) for half a second. Over a ~300-500 kB/s SSH link HERDR's client
    /// blocked on that output and held keys for 200-800 ms. The model link below
    /// drains 250 kB/s; paced frames must never queue a key change behind it.
    #[test]
    fn grabbed_picker_arrow_pulses_between_the_two_accent_levels_and_is_bold() {
        let mut field = Field::new(40, 5, 7);
        let (mut light_seen, mut dark_seen, mut seen) =
            (false, false, std::collections::BTreeSet::new());
        for now in (0..ARROW_PULSE_MS * 2).step_by(25) {
            let mut c = field.paint(now);
            let mut row = crate::widgets::Row::yes("Setting")
                .with_picker("Dense", "↕")
                .grabbed(true);
            row.preview_selected_only = false;
            let mut options = crate::widgets::Options::default();
            options.paint_menu(&mut c, &[row], 1, 4, 4);
            let i = c.cells.iter().position(|c| c.glyph == "↕").unwrap();
            let (x, y, cell) = (i % 40, i / 40, &c.cells[i]);
            let level = |brightness| {
                let theme = c.theme.as_ref().unwrap();
                theme
                    .accent(
                        x,
                        y,
                        &Accent {
                            brightness,
                            ..Accent::default()
                        },
                    )
                    .fg
            };
            let (light, dark) = (level(1000), level(600));
            assert!(cell.bold, "now={now}");
            for ch in 0..3 {
                let (lo, hi) = (light[ch].min(dark[ch]), light[ch].max(dark[ch]));
                assert!((lo..=hi).contains(&cell.fg[ch]), "now={now} {:?}", cell.fg);
            }
            light_seen |= cell.fg == light;
            dark_seen |= cell.fg == dark;
            seen.insert(cell.fg);
        }
        assert!(light_seen && dark_seen && seen.len() > 10, "{seen:?}");
    }
    #[test]
    fn star_sweep_is_paced_so_a_slow_link_never_queues_key_changes() {
        fn worst_backlog_ms(paced: bool) -> u64 {
            let mut field = Field::new(180, 30, 1967);
            let (mut shown, mut rotation, mut queued, mut worst) = (None::<Canvas>, 0, 0u64, 0);
            for (frame, now) in (4000..12000).step_by(50).enumerate() {
                let mut canvas = field.paint(now);
                // A held arrow key moves the highlighted row every frame.
                let label = if frame % 2 == 0 {
                    "> Home"
                } else {
                    "> Sessions"
                };
                canvas.text(4, 1, label, Cell::default(), false);
                let delta = if paced {
                    let (delta, next) =
                        canvas.paced_diff(shown.as_ref(), AMBIENT_CELLS_PER_FRAME, &mut rotation);
                    // Text is never deferred: the terminal shows this frame's row.
                    assert_eq!(
                        next.cells[180 + 4..180 + 14],
                        canvas.cells[180 + 4..180 + 14]
                    );
                    shown = Some(next);
                    delta
                } else {
                    let delta = canvas.diff(shown.as_ref());
                    shown = Some(canvas);
                    delta
                };
                if frame > 0 {
                    queued = (queued + delta.len() as u64).saturating_sub(12_500);
                    worst = worst.max(queued * 1000 / 250_000);
                }
            }
            // Deferred stars catch up once the sweep passes.
            let latest = field.paint(12_000);
            let mut rotation = 0;
            let mut shown = shown.unwrap();
            for _ in 0..4 {
                shown = latest
                    .paced_diff(Some(&shown), AMBIENT_CELLS_PER_FRAME, &mut rotation)
                    .1;
            }
            assert!(shown.cells == latest.cells || !paced);
            worst
        }
        assert!(
            worst_backlog_ms(false) >= 200,
            "model must reproduce the stall"
        );
        let paced = worst_backlog_ms(true);
        assert!(
            paced < 50,
            "key change queued {paced} ms behind star output"
        );
    }
    #[test]
    fn paced_diff_never_defers_text_or_cells_leaving_the_star_field() {
        let mut old = Canvas::new(4, 1);
        for i in 0..4 {
            old.cells[i].glyph = "*".into();
            old.ambient[i] = true;
        }
        let mut new = old.clone();
        for cell in &mut new.cells {
            cell.fg = [1, 2, 3];
        }
        new.text(3, 0, "x", Cell::default(), false);
        let mut rotation = 0;
        let (delta, shown) = new.paced_diff(Some(&old), 1, &mut rotation);
        assert_eq!(shown.cells[3].glyph, "x");
        assert_eq!(shown.cells.iter().filter(|c| c.fg == [1, 2, 3]).count(), 1);
        assert_eq!(delta.matches('*').count(), 1);
        // Round-robin: the next frame continues after the star written last.
        let (_, shown) = new.paced_diff(Some(&shown), 1, &mut rotation);
        assert_eq!(shown.cells.iter().filter(|c| c.fg == [1, 2, 3]).count(), 2);
        let (_, shown) = new.paced_diff(Some(&shown), 1, &mut rotation);
        assert_eq!(shown.cells, new.cells);
    }
    #[test]
    fn native_cursor_follows_frames_without_overriding_terminal_style_or_blink() {
        let mut frame = Canvas::new(20, 4);
        frame.cursor = Some((5, 2));
        let first = frame.diff(None);
        assert!(first.ends_with("\x1b[3;6H\x1b[?25h"));
        assert!(!first.contains(" q"));
        assert!(frame.diff(Some(&frame)).is_empty());
        let mut moved = frame.clone();
        moved.cursor = Some((6, 2));
        assert_eq!(moved.diff(Some(&frame)), "\x1b[3;7H");
        moved.cells[0].glyph = "*".into();
        let update = moved.diff(Some(&frame));
        assert!(update.ends_with("\x1b[3;7H"));
        assert!(!update.contains("?25")); // Background frames must not toggle blinking.
        frame.cursor = None;
        assert!(frame.diff(Some(&moved)).starts_with("\x1b[?25l"));
    }
    #[test]
    fn popups_beneath_the_top_one_fade_once_toward_the_background() {
        use crate::{dialog::Palette, popup::Panel};
        let palette = Palette::default();
        let base_color = [30, 20, 40];
        let mut field = Field::new(60, 24, 1967);
        field.background = Some(base_color);
        let base = field.paint(4200);
        assert_eq!(base.base, base_color);
        let fade = |cell: &Cell| Cell {
            fg: mix(cell.fg, base_color, UNFOCUSED_FADE),
            bg: cell.bg.map(|bg| mix(bg, base_color, UNFOCUSED_FADE)),
            ..cell.clone()
        };
        let inside = |p: &Panel, i: usize| {
            let (x, y) = (i % 60, i / 60);
            (p.x..p.x + p.width).contains(&x) && (p.y..p.y + p.height).contains(&y)
        };
        let panels = [
            Panel {
                x: 4,
                y: 3,
                width: 30,
                height: 12,
            },
            Panel {
                x: 20,
                y: 8,
                width: 30,
                height: 10,
            },
            Panel {
                x: 10,
                y: 2,
                width: 20,
                height: 4,
            },
        ];
        // Each popup painted alone, exactly as it looks when it is on top.
        let alone: Vec<Canvas> = panels
            .iter()
            .enumerate()
            .map(|(n, p)| {
                let mut c = base.clone();
                p.frame(&mut c, &format!("Popup {n}"), &palette);
                c
            })
            .collect();
        let mut c = base.clone();
        panels[0].frame(&mut c, "Popup 0", &palette);
        assert_eq!(c.cells, alone[0].cells, "a lone popup is unaffected");
        for top in 1..panels.len() {
            panels[top].frame(&mut c, &format!("Popup {top}"), &palette);
            for i in 0..c.cells.len() {
                // The topmost popup that covers this cell, if any.
                let owner = (0..=top).rev().find(|&n| inside(&panels[n], i));
                let expected = match owner {
                    None => base.cells[i].clone(),
                    Some(n) if n == top => alone[n].cells[i].clone(),
                    // Covered popups fade once, however many open above them.
                    Some(n) => fade(&alone[n].cells[i]),
                };
                assert_eq!(c.cells[i], expected, "top {top}, cell {i}");
            }
        }
        // Real dialogs stack the same way: the lower border dims, the top one does not.
        let mut c = base.clone();
        let mut lower = crate::dialog::Dialog::new("Lower", vec!["Below".into()], &["OK"]);
        lower.paint(&mut c, &palette);
        let corner = c.cells.iter().position(|cell| cell.glyph == "┌").unwrap();
        assert_eq!(c.cells[corner].fg, palette.accent);
        let mut upper = crate::dialog::Dialog::new("Upper", vec!["Top".into()], &["Close"]);
        upper.size = Some(crate::popup::Size::fit((60, 24), 24, 4));
        upper.paint(&mut c, &palette);
        assert_eq!(
            c.cells[corner].fg,
            mix(palette.accent, base_color, UNFOCUSED_FADE)
        );
        let top = c.cells.iter().position(|cell| cell.glyph == "U").unwrap();
        assert_eq!(c.cells[top].fg, palette.accent);
    }
    #[test]
    fn overlay_owns_native_cursor_instead_of_exposing_the_covered_editor() {
        let mut c = Canvas::new(20, 4);
        c.cursor = Some((2, 2));
        c.overlay(|_| {});
        assert!(c.cursor.is_none());
        c.overlay(|c| c.cursor = Some((4, 1)));
        assert_eq!(c.cursor, Some((4, 1)));
    }
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
    fn label_spaces_mask_baseline_and_twinkles_and_preserve_disabled_strike() {
        let mut c = Canvas::new(3, 1);
        for cell in &mut c.cells {
            cell.glyph = "*".into();
        }
        c.transient[1] = true;
        c.text(0, 0, "  ", Cell::default(), true);
        assert_eq!(c.cells[0].glyph, " ");
        assert_eq!(c.cells[1].glyph, " ");
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
    fn accent_labels_mask_spaces_even_over_an_existing_twinkle() {
        let mut c = Canvas::new(20, 2);
        c.cells[5].glyph = "*".into();
        c.transient[5] = true;
        c.accent_text(0, 0, "Login required", Accent::default());
        assert_eq!(c.cells[5].glyph, " ");
        assert!(c.foreground[5]);
        assert!(c.baseline_blocked[5]);
    }
    #[test]
    fn star_gradient_and_all_fades_blend_from_the_painted_background() {
        for background in [[26; 3], [40, 42, 54], [90, 35, 110]] {
            let mut f = Field::new(80, 24, 1967);
            f.background = Some(background);
            for now in (0..10000).step_by(20) {
                let c = f.paint(now);
                for cell in &c.cells {
                    assert_eq!(cell.bg, None);
                    if matches!(cell.glyph.as_str(), "*" | "." | "+") {
                        assert!(cell.fg.iter().zip(background).all(|(v, base)| *v >= base));
                    }
                }
            }
        }
        let mut f = Field::new(80, 24, 1967);
        let mut c = f.paint(10001);
        c.text(2, 3, "Login required", Cell::default(), false);
        assert_eq!(c.cells[3 * 80 + 7].bg, None);
        assert_eq!(mix([26; 3], [150, 180, 200], 0), [26; 3]);
    }
    #[test]
    fn dialog_occlusion_never_changes_background_evolution() {
        use crate::dialog::{Dialog, Palette};
        let mut covered = Field::new(80, 24, 1967);
        let mut control = Field::new(80, 24, 1967);
        let mut dialog = Dialog::new("Popup", vec!["Some text".into()], &["Close"]);
        for now in (0..12000).step_by(50) {
            let (w, h) = if now < 6000 { (80, 24) } else { (60, 18) };
            covered.resize(w, h);
            control.resize(w, h);
            let mut visible = covered.paint(now);
            let mut plain = control.paint(now);
            assert_eq!(visible.cells, plain.cells, "animation changed at {now}");
            visible.text(2, 2, "Underlying menu", Cell::default(), false);
            plain.text(2, 2, "Underlying menu", Cell::default(), false);
            if now % 2000 < 1500 {
                dialog.paint(&mut visible, &Palette::default());
                assert_ne!(visible.cells, plain.cells);
                assert_eq!(&visible.cells[(h - 1) * w..], &plain.cells[(h - 1) * w..]);
            }
            covered.apply_masks(&visible);
            control.apply_masks(&plain);
        }
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
