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
    /// How many leading entries of `roots` came from `--runtime` overrides.
    override_count: usize,
}

impl Runtime {
    /// Build the runtime search path. `overrides` (from `--runtime`) win, then
    /// the user config runtime, then `$HELIX_RUNTIME`.
    pub fn new(overrides: &[PathBuf]) -> Self {
        let mut roots: Vec<PathBuf> = overrides.iter().filter(|p| p.is_dir()).cloned().collect();
        let override_count = roots.len();

        if let Some(home) = home_dir() {
            let dir = home.join(".config/helix/runtime");
            if dir.is_dir() {
                roots.push(dir);
            }
        }
        if let Some(env) = std::env::var_os("HELIX_RUNTIME") {
            let dir = PathBuf::from(env);
            if dir.is_dir() {
                roots.push(dir);
            }
        }

        Runtime {
            roots,
            override_count,
        }
    }

    /// First existing match for a path relative to a runtime root.
    pub fn find_file(&self, rel: &Path) -> Option<PathBuf> {
        self.roots.iter().map(|r| r.join(rel)).find(|p| p.exists())
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Roots supplied via `--runtime` (highest priority).
    pub fn override_roots(&self) -> &[PathBuf] {
        &self.roots[..self.override_count]
    }

    /// Roots discovered from config dir / `$HELIX_RUNTIME` (below overrides).
    pub fn config_roots(&self) -> &[PathBuf] {
        &self.roots[self.override_count..]
    }
}

/// The user's home directory, if known.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create `<root>/grammars/<marker>.txt` so `find_file` has something to hit,
    /// returning the runtime root.
    fn seed_root(base: &Path, name: &str, marker: &str) -> PathBuf {
        let root = base.join(name);
        let dir = root.join("grammars");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.txt"), marker).unwrap();
        root
    }

    #[test]
    fn config_runtime_beats_helix_runtime_and_overrides_win() {
        let base = std::env::temp_dir().join(format!("dathan-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        let home = base.join("home");
        let config_rt = seed_root(&home, ".config/helix/runtime", "config");
        let env_rt = seed_root(&base, "env-runtime", "env");
        let override_rt = seed_root(&base, "override", "override");

        std::env::set_var("HOME", &home);
        std::env::set_var("HELIX_RUNTIME", &env_rt);

        let rel = Path::new("grammars/test.txt");

        // No override: the user config runtime wins over `$HELIX_RUNTIME`.
        let rt = Runtime::new(&[]);
        let found = rt.find_file(rel).unwrap();
        assert!(
            found.starts_with(&config_rt),
            "expected config copy, got {found:?}"
        );
        assert_eq!(fs::read_to_string(found).unwrap(), "config");

        // With an override, it wins over everything.
        let rt = Runtime::new(&[override_rt.clone()]);
        let found = rt.find_file(rel).unwrap();
        assert!(
            found.starts_with(&override_rt),
            "expected override copy, got {found:?}"
        );
        assert_eq!(fs::read_to_string(found).unwrap(), "override");
        assert_eq!(rt.override_roots(), &[override_rt]);

        std::env::remove_var("HELIX_RUNTIME");
        let _ = fs::remove_dir_all(&base);
    }
}
