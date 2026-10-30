//! Shared, nonblocking dialogs. Validation and cancellation never depend on focus.
use crate::{
    render::{Canvas, Cell},
    widgets::{NAME_MAX_CHARS, Row, TextInput},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
const INVALID: [u8; 3] = [215, 0, 0];
/// Option every dialog that shows a URL offers: its links alone, full screen.
pub const VIEW_LINK: &str = "View link fullscreen";
/// Scroll keys a dialog adds to its hints while its body scrolls.
pub const SCROLL_HINT: &str = "PgUp/PgDn: Scroll";
/// Shown under links the dialog no longer displays (replaced or withdrawn).
pub const STALE_LINK: &str = "Out of date: the popup no longer shows this link.";
/// The fullscreen link view, open over its dialog; the dialog keeps its selection.
#[derive(Clone, Debug, Default)]
struct LinkView {
    /// Links last drawn; kept, marked out of date, once the dialog stops showing them.
    shown: Vec<String>,
    offset: usize,
    /// Link rows that fit, and all link rows, as last painted.
    room: usize,
    total: usize,
}
#[derive(Clone, Debug)]
pub struct Palette {
    pub accent: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub secondary: [u8; 3],
    pub muted: [u8; 3],
    pub error: [u8; 3],
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: [255, 193, 140],
            panel: [16; 3],
            text: [255; 3],
            secondary: [188; 3],
            muted: [108; 3],
            error: INVALID,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub enum Rule {
    #[default]
    Any,
    AccountName,
    Percentage,
    /// Any non-empty value (a secret that has no format to check here).
    Required,
    Name,
    /// A HERDR Space label: the same rules as a new session's Space name.
    SpaceName,
    Rename(String),
    Exact(String),
}
#[derive(Clone, Debug)]
pub struct Field {
    pub label: String,
    pub input: TextInput,
    pub rule: Rule,
    pub excluded: Vec<String>,
}
impl Field {
    pub fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            input: TextInput::new(value.into()),
            rule: Rule::Any,
            excluded: vec![],
        }
    }
    pub fn validated(mut self, rule: Rule) -> Self {
        if matches!(
            rule,
            Rule::AccountName | Rule::Name | Rule::SpaceName | Rule::Rename(_)
        ) {
            self.input = self.input.limited(NAME_MAX_CHARS);
        }
        self.rule = rule;
        self
    }
    /// A masked field: the value is drawn as mask characters and never in clear.
    pub fn secret(mut self) -> Self {
        self.input = self.input.secret();
        self
    }
    pub fn paint_box(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        width: usize,
        palette: &Palette,
        focused: bool,
    ) {
        if width < 6 || y + 2 >= canvas.height {
            return;
        }
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        let field_style = Cell {
            fg: if self.valid() {
                palette.text
            } else {
                palette.error
            },
            ..style.clone()
        };
        let label = crate::widgets::fit_scrolling(
            &format!(" {} ", self.label),
            width - 4,
            crate::widgets::frame_time(canvas),
        );
        let label_width = UnicodeWidthStr::width(label.as_str());
        canvas.text(
            x,
            y,
            &format!(
                "┌─{}{}┐",
                label,
                "─".repeat((width - 3).saturating_sub(label_width))
            ),
            field_style.clone(),
            false,
        );
        canvas.text(x, y + 1, "│", field_style.clone(), false);
        canvas.text(x + width - 1, y + 1, "│", field_style.clone(), false);
        self.input
            .paint_focused(canvas, x + 1, y + 1, width - 3, self.valid(), focused);
        canvas.text(
            x,
            y + 2,
            &format!("└{}┘", "─".repeat(width - 2)),
            field_style.clone(),
            false,
        );
        if !self.valid() {
            for col in x..x + width {
                let cell = &mut canvas.cells[(y + 1) * canvas.width + col];
                if cell.bg.is_some_and(|bg| bg != palette.panel) {
                    cell.bg = Some(palette.error);
                } else {
                    cell.fg = palette.error;
                }
            }
        }
    }
    pub fn valid(&self) -> bool {
        let v = &self.input.value;
        if matches!(&self.rule, Rule::Rename(original) if v == original) {
            return true;
        }
        if self.excluded.contains(v) {
            return false;
        }
        match &self.rule {
            Rule::Any => true,
            Rule::Required => !v.is_empty(),
            Rule::Exact(expected) => v == expected,
            Rule::Percentage => v
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && (0. ..=100.).contains(&n)),
            Rule::AccountName => {
                !v.is_empty()
                    && v.len() <= NAME_MAX_CHARS
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            }
            Rule::SpaceName => crate::draft::valid_name(v),
            Rule::Name | Rule::Rename(_) => {
                !v.is_empty()
                    && v.trim() == v
                    && v.chars().count() <= NAME_MAX_CHARS
                    && !v.chars().any(char::is_control)
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Dialog {
    pub title: String,
    pub body: Vec<String>,
    pub fields: Vec<Field>,
    pub buttons: Vec<Row>,
    /// One-field forms keep their editor active while selection visits buttons only.
    pub selected: usize,
    pub offset: usize,
    pub visible: usize,
    pub wrapped: usize,
    pub busy: bool,
    pub compact: bool,
    pub status: Option<String>,
    pub size: Option<crate::popup::Size>,
    pub minimum_height: usize,
    pub minimum_width: usize,
    /// Body lines from this index on are a dim secondary group (identifiers, legends).
    pub dim_from: Option<usize>,
    /// Details layout: a wrapped `label  value` line continues under its value.
    pub hanging: bool,
    navigation: crate::widgets::SelectionNavigation,
    link_view: Option<LinkView>,
    /// Control hints shown inside the frame instead of the ones derived from its options.
    pub hints: Option<&'static [&'static str]>,
    /// Rows the frame reserved for fields and options when it was last sized.
    sized_reserved: usize,
}
pub enum Event {
    None,
    Dismiss,
    Choose(usize, Vec<String>),
}
impl Dialog {
    pub fn new(title: impl Into<String>, body: Vec<String>, buttons: &[&str]) -> Self {
        Self {
            title: title.into(),
            body,
            fields: vec![],
            buttons: buttons.iter().map(|s| Row::yes(*s)).collect(),
            selected: 0,
            offset: 0,
            visible: 0,
            wrapped: 0,
            busy: false,
            compact: false,
            status: None,
            size: None,
            minimum_height: 0,
            minimum_width: 0,
            dim_from: None,
            hanging: false,
            navigation: crate::widgets::SelectionNavigation::default(),
            link_view: None,
            sized_reserved: 0,
            hints: None,
        }
        .with_link_option()
    }
    fn with_link_option(mut self) -> Self {
        self.sync_link_option();
        self
    }
    /// URLs on screen in this dialog (body and status), in order, without repeats.
    pub fn links(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for link in self
            .body
            .iter()
            .chain(self.status.iter())
            .flat_map(|l| links_in(l))
        {
            if !out.contains(&link) {
                out.push(link);
            }
        }
        out
    }
    /// Whether the fullscreen link view is open over this dialog.
    pub fn link_open(&self) -> bool {
        self.link_view.is_some()
    }
    pub fn has_link_option(&self) -> bool {
        self.buttons.iter().any(|b| b.label == VIEW_LINK)
    }
    /// The controls this dialog shows on its own bottom row(s); every dialog and
    /// choice popup draws them through [`crate::popup::paint_frame_hints`].
    pub fn control_tokens(&self) -> Vec<&'static str> {
        if let Some(hints) = self.hints {
            return hints.to_vec();
        }
        let only = |label: &str| {
            self.fields.is_empty() && self.buttons.len() == 1 && self.buttons[0].label == label
        };
        let has = |label: &str| self.buttons.iter().any(|b| b.label == label);
        if self.compact && !self.has_link_option()
            || self.buttons.is_empty() && self.fields.is_empty()
            || only("Close")
        {
            vec!["Enter/Esc: Close"]
        } else if only("Cancel") {
            vec!["Enter/Esc: Cancel"]
        } else if only("Continue in background") {
            vec!["Esc: Close"]
        } else if !has("Cancel") && (has("Close") || has("Continue in background")) {
            vec!["▲/▼: Choose", "Enter: Select", "Esc: Close"]
        } else {
            vec!["▲/▼: Choose", "Enter: Select", "Esc: Cancel"]
        }
    }
    /// Offer [`VIEW_LINK`] exactly while the dialog shows a link (or its view is open),
    /// before the dismissing options; the cursor stays on the option it was on.
    fn sync_link_option(&mut self) {
        let slots = self.field_slots();
        let present = self.buttons.iter().position(|b| b.label == VIEW_LINK);
        let wanted = self.link_view.is_some() || !self.links().is_empty();
        match (present, wanted) {
            (None, true) => {
                let mut at = self
                    .buttons
                    .iter()
                    .position(|b| dismisses(&b.label))
                    .unwrap_or(self.buttons.len());
                if at > 0 && self.buttons[at - 1].separator {
                    at -= 1;
                }
                self.buttons.insert(at, Row::yes(VIEW_LINK));
                if self.selected >= slots + at && self.buttons.len() > 1 {
                    self.selected += 1;
                }
            }
            (Some(at), false) => {
                self.buttons.remove(at);
                if self.selected > slots + at {
                    self.selected -= 1;
                }
            }
            _ => {}
        }
    }
    /// A caller's own button index: [`VIEW_LINK`] belongs to the dialog, not to them.
    fn caller_button(&self, button: usize) -> usize {
        button - usize::from(self.buttons[..button].iter().any(|b| b.label == VIEW_LINK))
    }
    pub fn progress(title: &str) -> Self {
        let mut d = Self::new(title, vec!["Loading…".into()], &["Continue in background"]);
        d.busy = true;
        d.minimum_height = 16;
        d
    }
    /// Whether `text` is already on screen in this dialog (body or status).
    pub fn shows(&self, text: &str) -> bool {
        let text = text.trim();
        !text.is_empty()
            && (self.body.iter().any(|l| l.trim() == text)
                || self.status.as_deref().is_some_and(|s| s.trim() == text)
                || self
                    .body
                    .iter()
                    .any(|l| l.trim() == crate::messages::describe(text).trim()))
    }
    pub fn finish(&mut self) {
        self.busy = false;
        self.buttons = vec![Row::yes("Close")];
        self.selected = self.field_slots();
    }
    pub fn values(&self) -> Vec<String> {
        self.fields.iter().map(|f| f.input.value.clone()).collect()
    }
    fn editor(&self) -> Option<usize> {
        if self.fields.len() == 1 {
            Some(0)
        } else {
            (self.selected < self.fields.len()).then_some(self.selected)
        }
    }
    /// Make Cancel its own group: a bullet separator row directly above it.
    pub fn separate_cancel(&mut self) {
        if let Some(i) = self.buttons.iter().position(|b| b.label == "Cancel")
            && i > 0
            && !self.buttons[i - 1].separator
        {
            self.buttons.insert(i, Row::separator());
        }
    }
    fn selectable(&self, index: usize) -> bool {
        index < self.field_slots()
            || self
                .buttons
                .get(index - self.field_slots())
                .is_none_or(|b| !b.separator)
    }
    fn field_slots(&self) -> usize {
        if self.fields.len() == 1 {
            0
        } else {
            self.fields.len()
        }
    }
    fn dismiss_button(&self, index: usize) -> bool {
        self.buttons.get(index).is_some_and(|b| dismisses(&b.label))
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(i) = self.editor() {
            self.fields[i].input.insert(text);
            self.status = None;
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Event {
        self.key_with_wrap(key, key.kind != crossterm::event::KeyEventKind::Repeat)
    }
    pub fn key_with_wrap(&mut self, key: KeyEvent, wrap: bool) -> Event {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Event::None;
        }
        self.sync_link_option();
        if self.link_view.is_some() {
            self.link_key(key.code);
            return Event::None;
        }
        let slots = self.field_slots();
        let count = slots + self.buttons.len();
        let mask: Vec<bool> = (0..count).map(|i| self.selectable(i)).collect();
        self.selected = self.selected.min(count.saturating_sub(1));
        match key.code {
            KeyCode::Esc => return Event::Dismiss,
            KeyCode::Enter if self.buttons.is_empty() && self.fields.is_empty() => {
                return Event::Dismiss;
            }
            KeyCode::PageDown if self.wrapped > self.visible => {
                self.offset = (self.offset + self.visible.max(1))
                    .min(self.wrapped.saturating_sub(self.visible))
            }
            KeyCode::PageUp if self.wrapped > self.visible => {
                self.offset = self.offset.saturating_sub(self.visible.max(1))
            }
            KeyCode::Up | KeyCode::Down if count > 0 => {
                self.selected = self.navigation.step(
                    self.selected,
                    count,
                    key.code == KeyCode::Up,
                    wrap,
                    |i| mask.get(i).copied().unwrap_or(true),
                );
            }
            KeyCode::Home | KeyCode::End if self.editor().is_none() => {
                self.selected =
                    self.navigation
                        .boundary(self.selected, count, key.code == KeyCode::End, |i| {
                            mask.get(i).copied().unwrap_or(true)
                        });
            }
            KeyCode::Tab if count > 0 => {
                self.selected = (self.selected + 1) % count;
                while !self.selectable(self.selected) {
                    self.selected = (self.selected + 1) % count;
                }
            }
            KeyCode::Enter
                if self
                    .buttons
                    .get(self.selected.wrapping_sub(slots))
                    .is_some_and(|b| b.label == VIEW_LINK) =>
            {
                self.link_view = Some(LinkView::default());
            }
            KeyCode::Enter if self.selected >= slots => {
                let button = self.selected - slots;
                let cancel = self.dismiss_button(button);
                if self.buttons.get(button).is_some_and(|b| b.enabled)
                    && (cancel || self.fields.iter().all(Field::valid))
                    && (cancel || self.busy || self.visible > 0)
                {
                    return Event::Choose(self.caller_button(button), self.values());
                }
            }
            KeyCode::Enter => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Char(c) => self.paste(&c.to_string()),
            code => {
                if let Some(i) = self.editor() {
                    self.fields[i].input.edit(code);
                    self.status = None;
                }
            }
        }
        Event::None
    }
    /// Back (Esc, or Enter or Space on Back) returns to the dialog as it was
    /// left; arrows and PgUp/PgDn scroll links taller than the terminal.
    fn link_key(&mut self, code: KeyCode) {
        let Some(view) = &mut self.link_view else {
            return;
        };
        let last = view.total.saturating_sub(view.room);
        match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => {
                self.link_view = None;
                self.sync_link_option();
            }
            KeyCode::Up => view.offset = view.offset.saturating_sub(1),
            KeyCode::Down => view.offset = (view.offset + 1).min(last),
            KeyCode::PageUp => view.offset = view.offset.saturating_sub(view.room.max(1)),
            KeyCode::PageDown => view.offset = (view.offset + view.room.max(1)).min(last),
            _ => {}
        }
    }
    pub fn paint(&mut self, canvas: &mut Canvas, palette: &Palette) {
        self.sync_link_option();
        if self.link_view.is_some() {
            canvas.overlay(|canvas| self.paint_links(canvas, palette));
        } else {
            canvas.overlay(|canvas| self.paint_panel(canvas, palette));
        }
    }
    /// The links alone on the whole terminal, without frame or side padding: each
    /// wraps hard from column 0 to the last column so it copies as one block.
    fn paint_links(&mut self, canvas: &mut Canvas, palette: &Palette) {
        let current = self.links();
        let title = self.title.clone();
        let Some(view) = &mut self.link_view else {
            return;
        };
        let stale = current.is_empty();
        if !stale {
            view.shown = current;
        }
        let (width, height) = (canvas.width, canvas.height);
        if width == 0 || height == 0 {
            return;
        }
        canvas.open_popup(0, 0, width, height);
        let previous_palette = canvas.popup_palette.replace(palette.clone());
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        for row in 0..height {
            canvas.text(0, row, &" ".repeat(width), style.clone(), false);
        }
        let full = height >= 6;
        let top = usize::from(full) * 2;
        if full {
            // The popup title, in the border style a framed popup draws it in.
            let border = Cell {
                fg: palette.accent,
                ..style.clone()
            };
            let title =
                crate::widgets::fit_scrolling(&title, width, crate::widgets::frame_time(canvas));
            canvas.text(0, 0, &title, border, false);
        }
        let mut lines: Vec<(String, bool)> = vec![];
        for (i, link) in view.shown.iter().enumerate() {
            if i > 0 {
                lines.push((String::new(), false));
            }
            lines.extend(hard_wrap(link, width).into_iter().map(|l| (l, false)));
        }
        if stale {
            lines.push((String::new(), false));
            lines.extend(wrap(STALE_LINK, width).into_iter().map(|l| (l, true)));
        }
        let room = height.saturating_sub(top + if full { 3 } else { 1 }).max(1);
        view.room = room;
        view.total = lines.len();
        view.offset = view.offset.min(lines.len().saturating_sub(room));
        let shown = lines.len().saturating_sub(view.offset).min(room);
        for (i, (line, dim)) in lines.iter().skip(view.offset).take(shown).enumerate() {
            let fg = if *dim { palette.muted } else { palette.text };
            canvas.text(
                0,
                top + i,
                line,
                Cell {
                    fg,
                    ..style.clone()
                },
                false,
            );
        }
        let back = (top + shown + usize::from(full)).min(height - 1);
        Row::yes("Back").paint(canvas, 0, back, true, false);
        if full && back < height - 1 {
            let hint = if lines.len() > room {
                "PgUp/PgDn: Scroll"
            } else {
                "Esc: Back"
            };
            crate::widgets::paint_centered_hint(canvas, 0, height - 1, width, hint);
        }
        for cell in &mut canvas.cells {
            if cell.bg.is_none() {
                cell.bg = Some(palette.panel);
            }
            if matches!(cell.glyph.as_str(), "○" | "●") {
                cell.fg = palette.accent;
            } else if cell.fg == [238; 3] {
                cell.fg = palette.text;
            }
        }
        canvas.popup_palette = previous_palette;
    }
    fn paint_panel(&mut self, canvas: &mut Canvas, palette: &Palette) {
        let compact = !self.has_link_option() && self.compact
            || !self.busy
                && self.fields.is_empty()
                && self.buttons.len() == 1
                && self.buttons[0].label == "Close";
        let natural = self
            .body
            .iter()
            .chain(self.status.iter())
            .map(|s| UnicodeWidthStr::width(s.as_str()))
            .chain(self.fields.iter().map(|f| {
                (UnicodeWidthStr::width(f.label.as_str()) + 4)
                    .max(UnicodeWidthStr::width(f.input.value.as_str()) + 3)
                    .max(24)
            }))
            .chain(
                self.buttons
                    .iter()
                    .map(|b| UnicodeWidthStr::width(b.label.as_str()) + 2),
            )
            // Room for the controls on one row inside the frame, where it fits.
            .chain(std::iter::once(UnicodeWidthStr::width(
                self.control_tokens().join("   ").as_str(),
            )));
        let desired_width = crate::popup::content_width(canvas.width, &self.title, natural)
            .max(self.minimum_width.min(canvas.width.saturating_sub(4)));
        let width = self
            .size
            .filter(|s| s.viewport == (canvas.width, canvas.height))
            .map_or(desired_width, |s| s.width);
        let max_height = canvas.height.saturating_sub(4);
        let dim_from = self.dim_from.unwrap_or(usize::MAX);
        let hanging = self.hanging;
        let mut lines: Vec<(String, bool)> = self
            .body
            .iter()
            .enumerate()
            .flat_map(|(i, l)| {
                let l = crate::messages::describe(l);
                let width = width.saturating_sub(4);
                if hanging {
                    wrap_value(&l, width)
                } else {
                    wrap(&l, width)
                }
                .into_iter()
                .map(move |w| (w, i >= dim_from))
            })
            .collect();
        if let Some(status) = &self.status {
            lines.extend(
                wrap(&crate::messages::describe(status), width.saturating_sub(4))
                    .into_iter()
                    .map(|w| (w, false)),
            );
        }
        let mut normalized: Vec<(String, bool)> = Vec::new();
        for line in lines {
            if !line.0.trim().is_empty()
                || normalized.last().is_some_and(|l| !l.0.trim().is_empty())
            {
                normalized.push(line);
            }
        }
        while normalized.last().is_some_and(|l| l.0.trim().is_empty()) {
            normalized.pop();
        }
        let lines = normalized;
        // A selected-only button note (a reason) that does not fit beside its label
        // is shown below the buttons while it is selected; its rows stay reserved.
        let left = canvas.width.saturating_sub(width) / 2;
        let detached = usize::from(self.buttons.iter().any(|b| {
            b.preview_selected_only
                && b.preview.is_some()
                && b.preview_position(left + width - 2, left + 2).is_none()
        })) * 2;
        let controls = if compact || self.buttons.is_empty() && self.fields.is_empty() {
            2
        } else {
            self.fields.len() * 3
                + self.buttons.len()
                + detached
                + if self.fields.is_empty() { 3 } else { 4 }
        };
        // Control hints sit on the frame's own bottom row(s); a scrolling body adds
        // its scroll keys to them when the frame has room for the extra line.
        let tokens = self.control_tokens();
        let inner = width.saturating_sub(4);
        let plain = crate::popup::hint_lines(&tokens, inner);
        let scrolled = crate::popup::hint_lines(
            &std::iter::once(SCROLL_HINT)
                .chain(tokens.iter().copied())
                .collect::<Vec<_>>(),
            inner,
        );
        let base_reserved = controls + crate::popup::hint_space(plain.len());
        let scroll_extra =
            crate::popup::hint_space(scrolled.len()) - crate::popup::hint_space(plain.len());
        let viewport = (canvas.width, canvas.height);
        // Live body text never resizes an open frame, but options or fields that
        // appear later (a sign-in link's "View link fullscreen") grow it so they
        // stay inside the border; the frame never shrinks while it is open.
        let grow = self
            .size
            .is_some_and(|s| s.viewport == viewport && base_reserved > self.sized_reserved);
        if grow || self.size.is_none_or(|s| s.viewport != viewport) {
            let scroll = lines.len() > max_height.saturating_sub(base_reserved);
            let fit = crate::popup::Size::fit(
                viewport,
                width,
                (lines.len().max(1) + base_reserved + usize::from(scroll) * scroll_extra)
                    .max(self.minimum_height),
            );
            self.size = Some(match self.size.filter(|_| grow) {
                Some(open) => crate::popup::Size {
                    height: fit.height.max(open.height),
                    ..fit
                },
                None => fit,
            });
            self.sized_reserved = base_reserved;
        }
        let height = self.size.unwrap().height;
        // A live update that starts scrolling never grows the frame: the scroll keys
        // join the hints only when the sole body row survives the extra line.
        let scrolling = lines.len() > height.saturating_sub(base_reserved)
            && height > base_reserved + scroll_extra;
        let (hints, reserved) = if scrolling {
            (scrolled, base_reserved + scroll_extra)
        } else {
            (plain, base_reserved)
        };
        if width < 20 || max_height < reserved + 1 {
            self.visible = 0;
            self.wrapped = 1;
            canvas.text(
                0,
                1,
                "Resize to view dialog; Esc dismisses",
                Cell::default(),
                false,
            );
            return;
        }
        let previous_palette = canvas.popup_palette.replace(palette.clone());
        let x = (canvas.width - width) / 2;
        let y = (canvas.height - height) / 2;
        let body_height = height.saturating_sub(reserved).max(1);
        self.visible = body_height;
        self.wrapped = lines.len();
        self.offset = self.offset.min(lines.len().saturating_sub(body_height));
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        crate::popup::Panel {
            x,
            y,
            width,
            height,
        }
        .frame(canvas, &self.title, palette);
        for (i, (line, dim)) in lines.iter().skip(self.offset).take(body_height).enumerate() {
            let style = if *dim {
                Cell {
                    fg: palette.muted,
                    ..style.clone()
                }
            } else {
                style.clone()
            };
            canvas.text(x + 2, y + 1 + i, line, style, false);
        }
        if !compact {
            let mut row = y
                + 1
                + lines
                    .len()
                    .saturating_sub(self.offset)
                    .min(body_height)
                    .max(1)
                + usize::from(!self.fields.is_empty());
            for (index, field) in self.fields.iter().enumerate() {
                field.paint_box(
                    canvas,
                    x + 2,
                    row,
                    width - 4,
                    palette,
                    self.editor() == Some(index),
                );
                row += 3;
            }
            row += 1;
            let slots = self.field_slots();
            for (index, button) in self.buttons.iter().enumerate() {
                let mut button = button.clone();
                if !self.dismiss_button(index) && !self.fields.iter().all(Field::valid) {
                    button.enabled = false;
                }
                let selected = self.selected == slots + index;
                let inline = button.preview_selected_only
                    && button.preview_position(x + width - 2, x + 2).is_some();
                button.paint_with_preview(
                    canvas,
                    x + 2,
                    row + index,
                    selected,
                    false,
                    !button.preview_selected_only || inline,
                );
                if selected
                    && !inline
                    && button.preview_selected_only
                    && let Some(note) = button.preview.as_deref()
                {
                    let text = crate::widgets::fit_scrolling(
                        note,
                        width.saturating_sub(4),
                        crate::widgets::frame_time(canvas),
                    );
                    let at = row + self.buttons.len() + 1;
                    if at < (y + height - 1).saturating_sub(crate::popup::hint_space(hints.len())) {
                        button.paint_preview(
                            canvas,
                            x + width.saturating_sub(UnicodeWidthStr::width(text.as_str())) / 2,
                            at,
                            &text,
                            true,
                            false,
                            false,
                        );
                    }
                }
            }
        }
        crate::popup::paint_frame_hints(canvas, x + 2, y + height - 2, width - 4, &hints);
        for row in y..y + height {
            for col in x..x + width {
                let cell = &mut canvas.cells[row * canvas.width + col];
                if cell.bg.is_none() {
                    cell.bg = Some(palette.panel);
                }
                if matches!(cell.glyph.as_str(), "○" | "●") {
                    cell.fg = palette.accent;
                } else if cell.fg == [238; 3] {
                    cell.fg = palette.text;
                }
            }
        }
        canvas.popup_palette = previous_palette;
    }
}
/// Options that close a dialog without acting.
fn dismisses(label: &str) -> bool {
    matches!(label, "Cancel" | "Close" | "Continue in background")
}
/// `http://` and `https://` links in `text`, without trailing sentence punctuation
/// or unbalanced closing brackets.
pub fn links_in(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let start = ["https://", "http://"]
                .iter()
                .filter_map(|scheme| word.find(scheme))
                .min()?;
            let mut link = &word[start..];
            loop {
                let trimmed = link.trim_end_matches(['.', ',', ';', ':', '!', '?', '"', '\'']);
                let trimmed = [('(', ')'), ('[', ']'), ('<', '>')]
                    .iter()
                    .find(|(open, close)| {
                        trimmed.ends_with(*close)
                            && trimmed.matches(*close).count() > trimmed.matches(*open).count()
                    })
                    .map_or(trimmed, |_| &trimmed[..trimmed.len() - 1]);
                if trimmed == link {
                    break;
                }
                link = trimmed;
            }
            let rest = &link[link.find("://")? + 3..];
            (!rest.is_empty()).then(|| link.to_owned())
        })
        .collect()
}
/// Split `text` into rows of `width` columns (the last may be shorter), never at
/// a word: every character stays, in order, with nothing inserted.
pub fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut used = 0;
    for g in text.graphemes(true) {
        let w = UnicodeWidthStr::width(g);
        if used + w > width.max(1) && used > 0 {
            out.push(String::new());
            used = 0;
        }
        out.last_mut().unwrap().push_str(g);
        used += w;
    }
    out
}
/// Wrap a `label  value` line (label, then two or more spaces) with a hanging
/// indent: continuation lines start under the value. Other lines, and values
/// left with too little room, wrap normally.
fn wrap_value(text: &str, width: usize) -> Vec<String> {
    let indent = text.len() - text.trim_start().len();
    let column = text[indent..]
        .find("  ")
        .map(|label| indent + label)
        .and_then(|gap| text[gap..].find(|c| c != ' ').map(|value| gap + value));
    match column {
        Some(column)
            if column > indent
                && UnicodeWidthStr::width(&text[..column]) == column
                && column + 8 <= width =>
        {
            let (label, value) = text.split_at(column);
            wrap(value, width - column)
                .into_iter()
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        format!("{label}{line}")
                    } else {
                        format!("{}{line}", " ".repeat(column))
                    }
                })
                .collect()
        }
        _ => wrap(text, width),
    }
}
/// Prefer a space or dash; split a token only when the token exceeds the viewport.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    for paragraph in text.split('\n') {
        let mut remaining = paragraph;
        while UnicodeWidthStr::width(remaining) > width {
            let mut used = 0;
            let mut end = 0;
            let mut boundary = None;
            for (i, g) in remaining.grapheme_indices(true) {
                if used + UnicodeWidthStr::width(g) > width {
                    break;
                }
                used += UnicodeWidthStr::width(g);
                end = i + g.len();
                if g == " " || g == "-" {
                    boundary = Some(end);
                }
            }
            let end = boundary.filter(|n| *n > 0).unwrap_or(end);
            if end == 0 {
                break;
            }
            out.push(remaining[..end].trim_end().to_owned());
            remaining = remaining[end..].trim_start();
        }
        out.push(remaining.to_owned());
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }
    #[test]
    fn lone_close_is_hidden_but_background_progress_remains_actionable() {
        let mut d = Dialog::new("Information", vec!["Done".into()], &["Close"]);
        let mut c = Canvas::new(100, 30);
        d.paint(&mut c, &Palette::default());
        assert!(d.size.unwrap().width < 76);
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(!text.contains("○ Close") && !text.contains("● Close"));
        assert!(
            text.contains("Enter/Esc: Close"),
            "its controls sit inside its frame"
        );
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
        let mut d = Dialog::progress("Working");
        let mut c = Canvas::new(100, 30);
        d.paint(&mut c, &Palette::default());
        assert!(
            c.cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("Continue in background")
        );
    }
    #[test]
    fn async_form_fields_follow_description_with_exactly_one_blank_row() {
        let mut d = Dialog::new("Limits", vec!["Description".into()], &["Save", "Cancel"]);
        d.minimum_height = 16;
        d.fields = vec![Field::new("First", "20"), Field::new("Second", "30")];
        let mut c = Canvas::new(100, 30);
        d.paint(&mut c, &Palette::default());
        let lines: Vec<String> = c
            .cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        let body = lines
            .iter()
            .position(|r| r.contains("Description"))
            .unwrap();
        let field = lines.iter().position(|r| r.contains("First")).unwrap();
        assert_eq!(field, body + 2);
        let original = d.size.unwrap();
        d.fields[0].input.insert("longer value");
        d.paint(&mut Canvas::new(100, 30), &Palette::default());
        assert_eq!(d.size.unwrap().width, original.width);
    }
    #[test]
    fn live_content_and_completion_never_resize_an_open_dialog() {
        let mut d = Dialog::progress("Work");
        let palette = Palette::default();
        d.paint(&mut Canvas::new(90, 30), &palette);
        let initial = d.size.unwrap();
        d.body = (0..50).map(|i| format!("Update {i}")).collect();
        d.status = Some("Latest status".into());
        let mut canvas = Canvas::new(90, 30);
        d.paint(&mut canvas, &palette);
        assert_eq!(d.size.unwrap().height, initial.height);
        assert!(d.visible > 0 && d.wrapped > d.visible);
        assert!(
            canvas
                .cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("Continue in background")
        );
        d.finish();
        d.paint(&mut Canvas::new(90, 30), &palette);
        assert_eq!(d.size.unwrap().height, initial.height);
        d.paint(&mut Canvas::new(40, 20), &palette);
        assert_eq!(d.size.unwrap().viewport, (40, 20));
    }
    #[test]
    fn information_template_has_no_options_and_enter_or_escape_dismisses() {
        let mut d = Dialog::new("Information", vec!["Explanation".into()], &[]);
        d.paint(&mut Canvas::new(80, 24), &Palette::default());
        // Border, body, blank, its own hint row, border.
        assert_eq!(d.size.unwrap().height, 5);
        assert!(matches!(d.key(enter()), Event::Dismiss));
        assert!(matches!(
            d.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Event::Dismiss
        ));
    }
    #[test]
    fn title_uses_the_border_style_and_is_never_bold() {
        let mut d = Dialog::new("Session details", vec!["Body".into()], &["Close"]);
        let mut canvas = Canvas::new(60, 24);
        d.paint(&mut canvas, &Palette::default());
        let corner = canvas.cells.iter().position(|c| c.glyph == "┌").unwrap();
        let border = canvas.cells[corner].clone();
        for i in 0.."Session details".chars().count() {
            let cell = &canvas.cells[corner + 1 + i];
            assert_ne!(cell.glyph, "─");
            assert_eq!((cell.fg, cell.bg, cell.bold), (border.fg, border.bg, false));
        }
    }
    #[test]
    fn title_touches_herdr_corner_without_padding_and_keeps_right_corner() {
        for title in [
            "Popup Title",
            "A deliberately long title ".repeat(6).as_str(),
        ] {
            let mut d = Dialog::new(title, vec!["Body".into()], &["Close"]);
            let mut canvas = Canvas::new(60, 24);
            d.paint(&mut canvas, &Palette::default());
            let start = canvas.cells.iter().position(|c| c.glyph == "┌").unwrap();
            let row = canvas.cells[start..start + d.size.unwrap().width]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>();
            assert!(row.starts_with(&format!(
                "┌{}",
                crate::widgets::clip(title, d.size.unwrap().width - 2)
            )));
            assert!(row.ends_with('┐'));
            if title == "Popup Title" {
                assert!(row.starts_with("┌Popup Title──"));
            }
        }
    }
    #[test]
    fn limited_editors_and_only_focused_field_has_a_cursor() {
        let mut d = Dialog::new("Edit", vec!["Names".into()], &["Save", "Cancel"]);
        let mut name = Field::new("Name", "").validated(Rule::AccountName);
        name.excluded = vec!["duplicate".into()];
        d.fields.push(name);
        d.fields.push(Field::new("Other", "second"));
        d.paste("duplicate");
        assert!(!d.fields[0].valid());
        d.selected = 2;
        d.paint(&mut Canvas::new(80, 32), &Palette::default());
        assert!(matches!(d.key(enter()), Event::None));
        d.selected = 0;
        d.fields[0].input.clear();
        d.paste("abcdefghijklmnopqrstuvwxyz123456789");
        assert_eq!(d.fields[0].input.value, "abcdefghijklmnopqrstuvwxyz1234");
        assert!(d.fields[0].valid());
        for selected in 0..4 {
            d.selected = selected;
            let mut canvas = Canvas::new(80, 32);
            d.paint(&mut canvas, &Palette::default());
            assert_eq!(canvas.cursor.is_some(), selected < 2);
            assert!(canvas.cells.iter().all(|c| c.bg != Some([238; 3])));
            assert!(canvas.cells.iter().all(|c| c.glyph != "▶"));
        }
    }
    #[test]
    fn single_editor_stays_live_while_arrows_select_only_buttons() {
        let mut d = Dialog::new("Edit", vec![], &["Save", "Cancel"]);
        d.fields.push(Field::new("Name", ""));
        for (code, selected, letter) in [(KeyCode::Down, 1, 'a'), (KeyCode::Up, 0, 'b')] {
            d.key(KeyEvent::new(code, KeyModifiers::NONE));
            assert_eq!(d.selected, selected);
            d.key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
            let mut canvas = Canvas::new(80, 32);
            d.paint(&mut canvas, &Palette::default());
            assert!(canvas.cursor.is_some());
            assert!(canvas.cells.iter().all(|c| c.bg != Some([238; 3])));
            assert!(canvas.cells.iter().all(|c| c.glyph != "▶"));
        }
        assert_eq!(d.values(), vec!["ab"]);
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
    }
    #[test]
    fn dialog_hints_are_centered_and_theme_text_stays_stable_while_menu_animates() {
        let mut d = Dialog::new(
            "Confirm",
            (0..50).map(|i| format!("History row {i}")).collect(),
            &["Confirm", "Cancel"],
        );
        let mut field = crate::render::Field::new(80, 24, 1967);
        for now in [0, 1467, 4700] {
            let mut c = field.paint(now);
            d.paint(&mut c, &Palette::default());
            let index = c.cells.iter().position(|cell| cell.glyph == "P").unwrap();
            let cell = &c.cells[index];
            let size = d.size.unwrap();
            // The scroll keys lead the dialog's own hint row.
            let lines = crate::popup::hint_lines(
                &[SCROLL_HINT, "▲/▼: Choose", "Enter: Select", "Esc: Cancel"],
                size.width - 4,
            );
            assert!(lines[0].starts_with(SCROLL_HINT));
            let used = UnicodeWidthStr::width(lines[0].as_str());
            let x = (80 - size.width) / 2 + 2 + (size.width - 4 - used) / 2;
            assert_eq!(index % 80, x);
            assert_eq!(cell.fg, Palette::default().text);
            assert_ne!(cell.fg, crate::widgets::SECONDARY);
            assert!(!cell.bold);
            assert_eq!(cell.bg, Some(Palette::default().panel));
        }
    }
    #[test]
    fn wraps_words_dashes_and_unsplittable_graphemes() {
        assert_eq!(wrap("One two three", 8), vec!["One two", "three"]);
        assert_eq!(wrap("codex-personal", 8), vec!["codex-", "personal"]);
        let text = "/very/long/unbroken/αβ/é/👩‍💻";
        assert_eq!(wrap(text, 12).concat(), text);
    }
    #[test]
    fn cancel_works_before_reading_and_single_editor_never_takes_selection() {
        let mut d = Dialog::new(
            "Remove",
            (0..50).map(|i| format!("Session {i}")).collect(),
            &["Remove", "Cancel"],
        );
        d.fields.push(
            Field::new("Type codex-personal", "").validated(Rule::Exact("codex-personal".into())),
        );
        d.paint(&mut Canvas::new(60, 24), &Palette::default());
        assert!(matches!(d.key(enter()), Event::None));
        d.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        d.paste("codex-personal");
        assert_eq!(d.selected, 1);
        assert!(matches!(d.key(enter()), Event::Choose(1, _)));
        d.selected = 0;
        assert!(d.offset + d.visible < d.wrapped);
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
        for _ in 0..50 {
            d.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
    }
    #[test]
    fn details_values_wrap_with_a_hanging_indent_under_the_value_column() {
        assert_eq!(
            wrap_value(
                "Semantics:  remaining allowance in each provider window",
                30
            ),
            [
                "Semantics:  remaining",
                "            allowance in each",
                "            provider window"
            ]
        );
        // Indented identifiers hang at their own value column too.
        assert_eq!(
            wrap_value("  Account ID  claude:a-long-account-name", 30),
            ["  Account ID  claude:a-long-", "              account-name"]
        );
        // Prose, and values left with too little room, wrap from the left as before.
        for (text, width) in [("One two three", 8), ("Directory:  /a/b c/d e", 16)] {
            assert_eq!(wrap_value(text, width), wrap(text, width));
        }
        let body = vec![
            "Account:    ✻ work".to_owned(),
            "Semantics:  remaining allowance in each provider window, reported by the backend"
                .into(),
            String::new(),
            "Identifiers".into(),
            "  Account ID  claude:work".into(),
        ];
        for hanging in [false, true] {
            let mut d = Dialog::new("Account details", body.clone(), &["Close"]);
            d.hanging = hanging;
            let mut c = Canvas::new(44, 24);
            d.paint(&mut c, &Palette::default());
            let glyph = |x: usize, y: usize| c.cells[y * 44 + x].glyph.as_str();
            let left = (44 - d.size.unwrap().width) / 2 + 2;
            let y = (0..24).find(|&y| glyph(left, y) == "S").unwrap();
            let value = left + 12;
            assert_eq!(glyph(value, y), "r");
            assert_eq!(glyph(value, y - 1), "✻", "values share one column");
            let continuation = (left..44).find(|&x| glyph(x, y + 1) != " ").unwrap();
            assert_eq!(continuation, if hanging { value } else { left });
            if hanging {
                let last = (y + 1..24).take_while(|&y| glyph(value, y) != " ").last();
                assert!(
                    last.is_some_and(|last| last > y + 1),
                    "several continuations"
                );
                for row in y + 1..=last.unwrap() {
                    assert!((left..value).all(|x| glyph(x, row) == " "), "row {row}");
                }
            }
        }
    }
    #[test]
    fn invalid_field_stays_red_with_cancel_selected_and_short_body_has_no_scroll_hint() {
        let mut d = Dialog::new(
            "Remove",
            vec!["Keep history.".into()],
            &["Remove", "Cancel"],
        );
        d.fields.push(
            Field::new("Type codex-personal", "").validated(Rule::Exact("codex-personal".into())),
        );
        d.selected = 1;
        let mut c = Canvas::new(70, 24);
        d.paint(&mut c, &Palette::default());
        assert!(c.cells.iter().any(|c| c.fg == INVALID));
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(!text.contains("PgUp"));
        assert!(
            text.contains("Esc: Cancel"),
            "its controls sit inside its frame"
        );
        d.paste("codex-personal");
        assert!(d.fields[0].valid());
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn rows(c: &Canvas) -> Vec<String> {
        c.cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect()
    }
    /// A long OAuth link with every URL punctuation class.
    fn long_link() -> String {
        format!(
            "https://auth.example.invalid/oauth/authorize?client_id={}&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fcb&scope=openid+profile&state=a-b_c.d~e!$'()*,;=:@/?#frag",
            "x".repeat(200)
        )
    }
    #[test]
    fn links_keep_url_characters_and_drop_sentence_punctuation() {
        let link = long_link();
        assert_eq!(links_in(&link), vec![link.clone()]);
        assert_eq!(
            links_in("Open https://example.invalid/device. Then (see https://a.invalid/x_(y))."),
            ["https://example.invalid/device", "https://a.invalid/x_(y)"]
        );
        assert_eq!(
            links_in("<http://example.invalid/a>,"),
            ["http://example.invalid/a"]
        );
        assert!(links_in("No link: https:// or example.invalid").is_empty());
        for width in [1, 7, 40] {
            let rows = hard_wrap(&link, width);
            assert_eq!(rows.concat(), link);
            assert!(rows[..rows.len() - 1].iter().all(|r| r.len() == width));
        }
    }
    #[test]
    fn dialogs_offer_the_link_view_only_while_they_show_a_link() {
        let mut d = Dialog::new("Information", vec!["Plain text".into()], &["Close"]);
        assert!(!d.has_link_option());
        d.status = Some("Visit https://example.invalid/help".into());
        d.paint(&mut Canvas::new(80, 24), &Palette::default());
        let labels: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, [VIEW_LINK, "Close"]);
        assert_eq!(d.selected, 1, "the cursor stays on the option it was on");
        // A lone Close no longer hides the options: the link option is visible.
        let mut c = Canvas::new(80, 24);
        d.paint(&mut c, &Palette::default());
        assert!(rows(&c).iter().any(|r| r.contains(VIEW_LINK)));
        d.status = None;
        d.paint(&mut Canvas::new(80, 24), &Palette::default());
        assert!(!d.has_link_option());
        assert_eq!(d.selected, 0);
        // Callers keep their own button indices; the option sits before Cancel's group.
        let mut d = Dialog::new(
            "Confirm",
            vec!["https://example.invalid/device".into()],
            &["Confirm", "Cancel"],
        );
        d.separate_cancel();
        d.sync_link_option();
        let labels: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Confirm", VIEW_LINK, "", "Cancel"]);
        d.paint(&mut Canvas::new(80, 24), &Palette::default());
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
        d.selected = 3;
        assert!(matches!(d.key(enter()), Event::Choose(2, _)));
    }
    #[test]
    fn link_view_fills_every_column_at_any_width_and_reads_back_intact() {
        let link = long_link();
        let palette = Palette {
            accent: [1, 2, 3],
            panel: [4, 5, 6],
            text: [7, 8, 9],
            secondary: [10, 11, 12],
            muted: [13, 14, 15],
            error: [16, 17, 18],
        };
        for width in [40, 80, 160] {
            let mut d = Dialog::new(
                "Complete provider login",
                vec![link.clone(), "Code: ABCD-EFGH".into()],
                &["Continue in background"],
            );
            d.paint(&mut Canvas::new(width, 24), &palette);
            d.key(key(KeyCode::Up));
            assert_eq!(d.buttons[d.selected].label, VIEW_LINK);
            assert!(matches!(d.key(enter()), Event::None));
            assert!(d.link_open());
            let mut field = crate::render::Field::new(width, 24, 1967);
            let mut c = field.paint(4060);
            d.paint(&mut c, &palette);
            let text = rows(&c);
            // No frame anywhere, the title at column 0, the link from row 2.
            assert!(
                c.cells
                    .iter()
                    .all(|c| !"┌┐└┘│─".contains(c.glyph.as_str()) || c.glyph.is_empty())
            );
            assert!(text[0].starts_with("Complete provider login"));
            let link_rows: Vec<&String> = text[2..]
                .iter()
                .take_while(|r| !r.trim().is_empty())
                .collect();
            assert_eq!(link_rows.len(), link.len().div_ceil(width));
            for row in &link_rows[..link_rows.len() - 1] {
                assert_eq!(row.len(), width, "column 0 to the last column");
                assert!(!row.starts_with(' ') && !row.ends_with(' '));
            }
            let read: String = link_rows.iter().map(|r| r.trim_end()).collect();
            assert_eq!(read, link, "read back from the canvas at width {width}");
            // Theming: popup background everywhere, link in the text colour,
            // title in the border accent, Back as a selected popup option.
            assert!(c.cells.iter().all(|c| c.bg == Some(palette.panel)));
            for y in 2..2 + link_rows.len() {
                for cell in &c.cells[y * width..(y + 1) * width] {
                    if !cell.glyph.trim().is_empty() {
                        assert_eq!(
                            (cell.fg, cell.bold, cell.strike),
                            (palette.text, false, false)
                        );
                    }
                }
            }
            assert_eq!(c.cells[0].fg, palette.accent);
            let back = 2 + link_rows.len() + 1;
            assert!(text[back].starts_with("● Back"));
            assert_eq!(c.cells[back * width].fg, palette.accent);
            assert_eq!(c.cells[back * width + 2].fg, palette.text);
            assert!(text[23].contains("Esc: Back"));
            // Left never closes the link view.
            assert!(matches!(d.key(key(KeyCode::Left)), Event::None));
            assert!(d.link_open());
            // Back and Esc return to the dialog, cursor where it was.
            for code in [KeyCode::Enter, KeyCode::Esc] {
                assert!(matches!(d.key(key(code)), Event::None));
                assert!(!d.link_open());
                assert_eq!(d.buttons[d.selected].label, VIEW_LINK);
                let mut c = Canvas::new(width, 24);
                d.paint(&mut c, &palette);
                assert!(rows(&c).iter().any(|r| r.contains('┌')));
                assert!(matches!(d.key(enter()), Event::None));
                assert!(d.link_open());
            }
        }
    }
    #[test]
    fn link_view_rewraps_on_resize_and_follows_or_marks_a_changed_link() {
        let mut d = Dialog::new(
            "Login",
            vec!["https://example.invalid/first".into()],
            &["Cancel"],
        );
        d.selected = 0;
        d.key(enter());
        assert!(d.link_open());
        let link = long_link();
        d.body = vec![link.clone()];
        for width in [50, 30] {
            let mut c = Canvas::new(width, 40);
            d.paint(&mut c, &Palette::default());
            let text = rows(&c);
            assert_eq!(text[2].len(), width);
            let read: String = text[2..]
                .iter()
                .take_while(|r| !r.trim().is_empty())
                .map(|r| r.trim_end())
                .collect();
            assert_eq!(read, link, "the view shows the current link");
        }
        // The dialog stops showing it: the last link stays, marked out of date.
        d.body = vec!["Login complete".into()];
        let mut c = Canvas::new(60, 40);
        d.paint(&mut c, &Palette::default());
        let text = rows(&c);
        assert!(text[2].starts_with("https://auth.example.invalid/"));
        let y = text
            .iter()
            .position(|r| r.starts_with("Out of date"))
            .unwrap();
        assert_eq!(c.cells[y * 60].fg, Palette::default().muted);
        d.key(key(KeyCode::Esc));
        assert!(!d.link_open() && !d.has_link_option());
        // Taller than the terminal: PgDn scrolls and the hint says so.
        d.body = vec![link.clone()];
        d.sync_link_option();
        d.selected = 0;
        d.key(key(KeyCode::Enter));
        assert!(d.link_open());
        let mut c = Canvas::new(24, 12);
        d.paint(&mut c, &Palette::default());
        assert!(rows(&c)[11].contains("PgUp/PgDn"));
        d.key(key(KeyCode::PageDown));
        let mut c = Canvas::new(24, 12);
        d.paint(&mut c, &Palette::default());
        assert_eq!(rows(&c)[2], link[24 * 7..24 * 8]);
    }
    /// Every dialog kind draws its controls itself: centered on the last row(s)
    /// inside its own frame, keys in the panel's text colour and actions muted,
    /// with nothing below the frame.
    #[test]
    fn every_dialog_kind_centers_its_hints_on_its_own_bottom_rows() {
        let palette = Palette::default();
        let mut form = Dialog::new(
            "Rename account",
            vec!["Name it.".into()],
            &["Save", "Cancel"],
        );
        form.fields.push(Field::new("Name", "work"));
        let mut compact = Dialog::new("Saved", vec!["Done.".into()], &["Close"]);
        compact.compact = true;
        let mut custom = Dialog::new("New Space name", vec!["Name it.".into()], &["Continue"]);
        custom.hints = Some(&["Enter: Accept", "Esc: Cancel", "Ctrl-U: Clear"]);
        let link = Dialog::new(
            "Codex login",
            vec!["https://example.invalid/device".into()],
            &["Continue in background"],
        );
        for (mut d, hint) in [
            (
                Dialog::new("Info", vec!["Nothing to choose.".into()], &[]),
                "Enter/Esc: Close",
            ),
            (compact, "Enter/Esc: Close"),
            (
                Dialog::new("Done", vec!["All set.".into()], &["Close"]),
                "Enter/Esc: Close",
            ),
            (
                Dialog::new("Checking", vec!["Looking…".into()], &["Cancel"]),
                "Enter/Esc: Cancel",
            ),
            (
                Dialog::new("Deregister", vec!["Sure?".into()], &["Confirm", "Cancel"]),
                "▲/▼: Choose   Enter: Select   Esc: Cancel",
            ),
            (Dialog::progress("Operation progress"), "Esc: Close"),
            (link, "▲/▼: Choose   Enter: Select   Esc: Close"),
            (form, "▲/▼: Choose   Enter: Select   Esc: Cancel"),
            (custom, "Enter: Accept   Esc: Cancel   Ctrl-U: Clear"),
        ] {
            for (width, height) in [(60, 20), (80, 24), (120, 30)] {
                let mut c = Canvas::new(width, height);
                d.size = None;
                d.paint(&mut c, &palette);
                let rows = rows(&c);
                let bottom = rows.iter().rposition(|r| r.contains('└')).unwrap();
                let top = rows.iter().position(|r| r.contains('┌')).unwrap();
                let left = rows[top].chars().position(|c| c == '┌').unwrap();
                let right = rows[top].chars().position(|c| c == '┐').unwrap();
                let row: Vec<char> = rows[bottom - 1].chars().collect();
                let inside: String = row[left + 2..right - 1].iter().collect();
                assert_eq!(inside.trim(), hint, "{} {width}: {rows:#?}", d.title);
                let lead = inside.chars().take_while(|c| *c == ' ').count();
                let trail = inside.chars().rev().take_while(|c| *c == ' ').count();
                assert!(lead.abs_diff(trail) <= 1, "{} {width}: centered", d.title);
                // One blank row separates the hints from the content above them.
                assert!(
                    rows[bottom - 2].chars().collect::<Vec<_>>()[left + 1..right]
                        .iter()
                        .all(|c| *c == ' '),
                    "{} {width}: {rows:#?}",
                    d.title
                );
                // Theme colours: keys in the panel's text colour, actions muted.
                let x = left + 2 + lead;
                let key = &c.cells[(bottom - 1) * width + x];
                assert_eq!((key.fg, key.bg), (palette.text, Some(palette.panel)));
                let colon = hint.find(": ").unwrap();
                let action = &c.cells[(bottom - 1) * width + x + hint[..colon].chars().count() + 2];
                assert_eq!(action.fg, palette.muted, "{}", d.title);
                // Nothing of the dialog below its frame.
                assert!(rows[bottom + 1..].iter().all(|r| r.trim().is_empty()));
            }
        }
    }
}
