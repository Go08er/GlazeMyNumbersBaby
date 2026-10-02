//! XDG base directories (same rules as GLib: the environment variable if it
//! holds an absolute path, else the default under `$HOME`). Inside Flatpak
//! these point into `~/.var/app/<id>/`.

use std::path::PathBuf;

fn xdg(var: &str, fallback: &str) -> PathBuf {
    if let Some(v) = std::env::var_os(var) {
        let p = PathBuf::from(v);
        if p.is_absolute() {
            return p;
        }
    }
    home().join(fallback)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
}

/// `$XDG_CONFIG_HOME/<app>`.
pub fn config_dir(app: &str) -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join(app)
}

/// `$XDG_CACHE_HOME/<app>`.
pub fn cache_dir(app: &str) -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join(app)
}
