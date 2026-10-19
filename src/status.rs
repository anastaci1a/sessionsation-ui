//! Shared status line: cached local clock, host and Unicode-safe path fitting.
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct Status {
    host: String,
    home: String,
    second: i64,
    clock: String,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            host: std::env::var("HOSTNAME")
                .unwrap_or_default()
                .split('.')
                .next()
                .unwrap_or("")
                .chars()
                .take(12)
                .collect(),
            home: std::env::var("HOME").unwrap_or_default(),
            second: i64::MIN,
            clock: String::new(),
        }
    }
}
impl Status {
    pub fn line(&mut self, seconds: i64, path: &Path, width: usize) -> String {
        if self.second != seconds {
            let time = seconds as libc::time_t;
            let mut broken = std::mem::MaybeUninit::<libc::tm>::uninit();
            let mut buffer = [0i8; 64];
            // libc writes into fixed, valid storage and does not retain pointers.
            unsafe {
                if !libc::localtime_r(&time, broken.as_mut_ptr()).is_null() {
                    let n = libc::strftime(
                        buffer.as_mut_ptr(),
                        buffer.len(),
                        c"%a %b %d %-H:%M:%S".as_ptr(),
                        broken.as_ptr(),
                    );
                    self.clock = String::from_utf8_lossy(std::slice::from_raw_parts(
                        buffer.as_ptr().cast::<u8>(),
                        n,
                    ))
                    .into_owned();
                }
            }
            self.second = seconds;
        }
        let path = path.to_string_lossy();
        let path = if !self.home.is_empty()
            && (path == self.home || path.starts_with(&format!("{}/", self.home)))
        {
            format!("~{}", &path[self.home.len()..])
        } else {
            path.into_owned()
        };
        fit(&self.clock, &self.host, &path, width)
    }
}
fn fit(clock: &str, host: &str, path: &str, width: usize) -> String {
    let prefix = format!(" {clock} | {host} | ");
    let available = width.saturating_sub(UnicodeWidthStr::width(prefix.as_str()) + 1);
    if available <= 3 {
        return format!(" {clock}");
    }
    if UnicodeWidthStr::width(path) <= available {
        return format!("{prefix}{path}");
    }
    let mut suffix = Vec::new();
    let mut used = 3;
    for g in path.graphemes(true).rev() {
        let w = UnicodeWidthStr::width(g);
        if used + w > available {
            break;
        }
        suffix.push(g);
        used += w;
    }
    format!(
        "{prefix}...{}",
        suffix.into_iter().rev().collect::<String>()
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_preserves_path_tail_and_fits_wide_glyphs() {
        let s = fit(
            "Thu Oct 15 12:30:00",
            "ExamplePC",
            "/long/long/long/界/leaf",
            50,
        );
        assert!(s.ends_with("界/leaf"));
        assert!(s.contains("..."));
        assert!(UnicodeWidthStr::width(s.as_str()) <= 50);
        assert_eq!(fit("12:00", "ExamplePC", "/path", 12), " 12:00");
    }
}
