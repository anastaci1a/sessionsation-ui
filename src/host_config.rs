//! Shared read-only location for the HERDR configuration used by this popup.
use std::path::PathBuf;
pub fn path() -> PathBuf {
    std::env::var_os("HERDR_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
                });
            base.join("herdr/config.toml")
        })
}
