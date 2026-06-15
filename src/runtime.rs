//! Discovery of Helix runtime directories.
//!
//! Helix ships grammars at `runtime/grammars/<lang>.{dylib,so}` and queries at
//! `runtime/queries/<lang>/{highlights,injections,locals}.scm`. Several runtime
//! trees may coexist; user config shadows the source distribution.

use std::path::{Path, PathBuf};

/// Platform shared-library extension for compiled grammars.
#[cfg(target_os = "macos")]
pub const DYLIB_EXT: &str = "dylib";
#[cfg(all(unix, not(target_os = "macos")))]
pub const DYLIB_EXT: &str = "so";
#[cfg(windows)]
pub const DYLIB_EXT: &str = "dll";

/// An ordered set of runtime roots. Earlier roots take precedence.
pub struct Runtime {
    roots: Vec<PathBuf>,
}

impl Runtime {
    /// Build the runtime search path. `overrides` (from `--runtime`) win, then
    /// `$HELIX_RUNTIME`, then the user config runtime.
    pub fn new(overrides: &[PathBuf]) -> Self {
        let mut roots: Vec<PathBuf> = overrides.to_vec();

        if let Some(env) = std::env::var_os("HELIX_RUNTIME") {
            roots.push(PathBuf::from(env));
        }
        if let Some(home) = home_dir() {
            roots.push(home.join(".config/helix/runtime"));
        }

        roots.retain(|p| p.is_dir());
        Runtime { roots }
    }

    /// First existing match for a path relative to a runtime root.
    pub fn find_file(&self, rel: &Path) -> Option<PathBuf> {
        self.roots.iter().map(|r| r.join(rel)).find(|p| p.exists())
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

/// The user's home directory, if known.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
