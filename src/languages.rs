//! Language registry and the `tree_house::LanguageLoader` implementation.
//!
//! Parses Helix's `languages.toml` into a registry, detects a file's language,
//! resolves injection markers, and lazily compiles a `LanguageConfig` per
//! language. Compiled configs are `configure`d against a single global,
//! append-only list of recognized scope names so a `Highlight` index maps back
//! to the exact dotted capture name (e.g. `keyword.control.conditional`).

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use ropey::RopeSlice;
use serde::Deserialize;
use toml::Value;
use tree_house::highlighter::Highlight;
use tree_house::{InjectionLanguageMarker, Language, LanguageConfig, LanguageLoader};

use crate::grammar;
use crate::queries::read_query;
use crate::runtime::Runtime;

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default)]
    language: Vec<RawLang>,
}

#[derive(Debug, Deserialize)]
struct RawLang {
    name: String,
    #[serde(default)]
    grammar: Option<String>,
    #[serde(default, rename = "injection-regex")]
    injection_regex: Option<String>,
    #[serde(default, rename = "file-types")]
    file_types: Vec<FileType>,
    #[serde(default)]
    shebangs: Vec<String>,
}

/// A `file-types` entry: either a bare extension or a `{ glob = .. }` /
/// `{ suffix = .. }` table.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FileType {
    Extension(String),
    Special(HashMap<String, String>),
}

/// Processed per-language data.
struct LangData {
    name: String,
    grammar: String,
    injection_regex: Option<Regex>,
    file_types: Vec<FileType>,
}

pub struct Loader {
    rt: Runtime,
    langs: Vec<LangData>,
    configs: Vec<OnceCell<Option<LanguageConfig>>>,
    by_extension: HashMap<String, Language>,
    by_name: HashMap<String, Language>,
    by_shebang: HashMap<String, Language>,
    /// Global, append-only list of recognized scope names. The index of a name
    /// is the `Highlight` value reported for captures of that name.
    recognized: RefCell<Vec<String>>,
}

impl Loader {
    /// Build a registry from an already-merged `languages.toml` value.
    pub fn new(rt: Runtime, config: Value) -> Result<Self> {
        let raw: RawConfig = config.try_into().context("interpreting languages.toml")?;

        let mut langs = Vec::with_capacity(raw.language.len());
        let mut by_extension = HashMap::new();
        let mut by_name = HashMap::new();
        let mut by_shebang = HashMap::new();

        for (i, l) in raw.language.into_iter().enumerate() {
            let lang = Language::new(u32::try_from(i).expect("language index fits in u32"));
            by_name.insert(l.name.clone(), lang);

            for ft in &l.file_types {
                if let FileType::Extension(ext) = ft {
                    by_extension.insert(ext.clone(), lang);
                }
            }
            for sb in &l.shebangs {
                by_shebang.insert(sb.clone(), lang);
            }

            let injection_regex = l.injection_regex.as_deref().and_then(|s| {
                Regex::new(s)
                    .map_err(|e| eprintln!("dathan: bad injection-regex for '{}': {e}", l.name))
                    .ok()
            });

            langs.push(LangData {
                grammar: l.grammar.unwrap_or_else(|| l.name.clone()),
                name: l.name,
                injection_regex,
                file_types: l.file_types,
            });
        }

        let configs = (0..langs.len()).map(|_| OnceCell::new()).collect();

        Ok(Self {
            rt,
            langs,
            configs,
            by_extension,
            by_name,
            by_shebang,
            recognized: RefCell::new(Vec::new()),
        })
    }

    /// Resolve a `Highlight` back to its dotted scope name.
    pub fn scope_name(&self, highlight: Highlight) -> String {
        self.recognized
            .borrow()
            .get(highlight.idx())
            .cloned()
            .unwrap_or_default()
    }

    /// Intern a capture name into the global recognized list (append-only) and
    /// return its `Highlight` index.
    fn intern(&self, name: &str) -> Highlight {
        let mut rec = self.recognized.borrow_mut();
        let idx = rec.iter().position(|n| n == name).unwrap_or_else(|| {
            rec.push(name.to_string());
            rec.len() - 1
        });
        Highlight::new(u32::try_from(idx).expect("capture index fits in u32"))
    }

    /// Compile grammar + queries into a configured `LanguageConfig`.
    fn compile(&self, lang: Language) -> Option<LanguageConfig> {
        let data = &self.langs[lang.idx()];

        let grammar = match grammar::load(&data.grammar, &self.rt) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("dathan: {e:#}");
                return None;
            }
        };

        let highlights = read_query(&self.rt, &data.name, "highlights.scm");
        let injections = read_query(&self.rt, &data.name, "injections.scm");
        let locals = read_query(&self.rt, &data.name, "locals.scm");

        let config = match LanguageConfig::new(grammar, &highlights, &injections, &locals) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("dathan: failed to compile queries for '{}': {e}", data.name);
                return None;
            }
        };

        config.configure(|name| Some(self.intern(name)));
        Some(config)
    }

    pub fn language_for_name(&self, name: &str) -> Option<Language> {
        self.by_name.get(name).copied()
    }

    /// Detect by glob/suffix `file-types` entries first (the longest matching
    /// pattern wins), then by extension, mirroring Helix's precedence.
    pub fn language_for_filename(&self, path: &Path) -> Option<Language> {
        let path_str = path.to_str().unwrap_or_default();
        let name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_default();

        // (pattern length, language index) of the best glob/suffix hit so far.
        let mut best: Option<(usize, usize)> = None;
        for (i, data) in self.langs.iter().enumerate() {
            for ft in &data.file_types {
                if let FileType::Special(map) = ft {
                    let hit = map
                        .get("glob")
                        .filter(|g| glob_match(g, path_str))
                        .or_else(|| map.get("suffix").filter(|s| name.ends_with(s.as_str())));
                    if let Some(pattern) = hit {
                        if best.is_none_or(|(len, _)| pattern.len() > len) {
                            best = Some((pattern.len(), i));
                        }
                    }
                }
            }
        }
        if let Some((_, i)) = best {
            return Some(Language::new(
                u32::try_from(i).expect("language index fits in u32"),
            ));
        }

        path.extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| self.by_extension.get(ext).copied())
    }

    /// Resolve an injected-language token: exact name first, then the longest
    /// `injection-regex` match (mirrors Helix's `language_for_match`).
    pub fn language_for_match(&self, text: &str) -> Option<Language> {
        if let Some(lang) = self.language_for_name(text) {
            return Some(lang);
        }
        let mut best_len = 0;
        let mut best = None;
        for (i, data) in self.langs.iter().enumerate() {
            if let Some(re) = &data.injection_regex {
                if let Some(m) = re.find(text) {
                    let len = m.end() - m.start();
                    if len > best_len {
                        best_len = len;
                        best = Some(Language::new(
                            u32::try_from(i).expect("language index fits in u32"),
                        ));
                    }
                }
            }
        }
        best
    }

    /// Resolve a language from a `#!` shebang line.
    pub fn language_for_shebang(&self, line: &str) -> Option<Language> {
        let rest = line.strip_prefix("#!")?.trim_start();
        let mut tokens = rest.split_whitespace();
        let first = tokens.next()?;
        let interpreter = if first.rsplit(['/', '\\']).next() == Some("env") {
            tokens.next()?
        } else {
            first
        };
        let name = interpreter.rsplit(['/', '\\']).next()?;
        self.by_shebang.get(name).copied()
    }
}

impl LanguageLoader for Loader {
    fn language_for_marker(&self, marker: InjectionLanguageMarker) -> Option<Language> {
        match marker {
            InjectionLanguageMarker::Name(name) => self.language_for_name(name),
            InjectionLanguageMarker::Match(text) => self.language_for_match(&slice_to_string(text)),
            InjectionLanguageMarker::Filename(text) => {
                self.language_for_filename(Path::new(&slice_to_string(text)))
            }
            InjectionLanguageMarker::Shebang(text) => {
                let token = slice_to_string(text);
                self.by_shebang.get(&token).copied()
            }
        }
    }

    fn get_config(&self, lang: Language) -> Option<&LanguageConfig> {
        self.configs[lang.idx()]
            .get_or_init(|| self.compile(lang))
            .as_ref()
    }
}

fn slice_to_string(slice: RopeSlice) -> String {
    String::from(slice)
}

/// Match a Helix `file-types` glob against a path, mirroring how Helix uses
/// `globset` with default settings: `*` matches any characters, `/` included.
/// A pattern starting with `/` is anchored to the whole path; any other
/// pattern matches from a path component boundary (Helix prepends `*/`), plus
/// from the start so a bare filename like `.gitignore` still matches.
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.starts_with('/') {
        return wildcard_match(pattern, path);
    }
    std::iter::once(0)
        .chain(path.match_indices('/').map(|(i, _)| i + 1))
        .any(|start| wildcard_match(pattern, &path[start..]))
}

/// Wildcard match where `*` matches any (possibly empty) run of characters
/// and everything else is literal. Greedy with backtracking. Operates on
/// bytes, which is UTF-8 safe because `*` is the only metacharacter and it
/// matches arbitrary byte runs.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0, 0);
    // Position after the most recent `*` and the text position it matched at.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi + 1, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            // Backtrack: let the last `*` swallow one more byte.
            pi = sp;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&b| b == b'*')
}

/// Deep-merge an `overlay` `languages.toml` onto a `base`, the way Helix merges
/// the user config over the default: `[[language]]`/`[[grammar]]` arrays are
/// merged by `name` (overlay entries override/extend matching base entries and
/// append new ones); other keys are overridden by the overlay.
pub fn merge_configs(base: &str, overlay: Option<&str>) -> Result<Value> {
    let base: Value = toml::from_str(base).context("parsing base languages.toml")?;
    match overlay {
        None => Ok(base),
        Some(overlay) => {
            let overlay: Value = toml::from_str(overlay).context("parsing user languages.toml")?;
            Ok(merge_values(base, overlay))
        }
    }
}

fn merge_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Table(mut base), Value::Table(overlay)) => {
            for (key, ov) in overlay {
                let merged = match base.remove(&key) {
                    Some(bv) if key == "language" || key == "grammar" => merge_named_array(bv, ov),
                    Some(bv) => merge_values(bv, ov),
                    None => ov,
                };
                base.insert(key, merged);
            }
            Value::Table(base)
        }
        // Scalars and arrays without a merge key: the overlay wins.
        (_, overlay) => overlay,
    }
}

/// Merge two arrays of tables keyed by their `name` field.
fn merge_named_array(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Array(mut base), Value::Array(overlay)) => {
            for item in overlay {
                let pos = item.get("name").and_then(Value::as_str).and_then(|name| {
                    base.iter()
                        .position(|b| b.get("name").and_then(Value::as_str) == Some(name))
                });
                match pos {
                    Some(i) => base[i] = merge_values(base[i].clone(), item),
                    None => base.push(item),
                }
            }
            Value::Array(base)
        }
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        // Bare filename patterns, at any depth or as the whole path.
        assert!(glob_match("Makefile", "Makefile"));
        assert!(glob_match("Makefile", "src/Makefile"));
        assert!(!glob_match("Makefile", "makefile"));
        assert!(!glob_match("Makefile", "notMakefile"));
        // `*` anywhere, including patterns Helix ships.
        assert!(glob_match("*.toml", "dir/Cargo.toml"));
        assert!(glob_match(".*ignore", ".gitignore"));
        assert!(glob_match(".env.*", "repo/.env.local"));
        assert!(glob_match("_environment-*", "_environment-prod"));
        assert!(!glob_match("*.rs", "main.py"));
        // Path-qualified patterns match trailing components.
        assert!(glob_match(".git/config", "repo/.git/config"));
        assert!(glob_match(".git/config", ".git/config"));
        assert!(!glob_match(".git/config", "config"));
        assert!(glob_match(
            ".git/modules/*/config",
            ".git/modules/sub/config"
        ));
        // Absolute patterns anchor to the whole path.
        assert!(glob_match("/etc/hosts", "/etc/hosts"));
        assert!(!glob_match("/etc/hosts", "/home/x/etc/hosts"));
    }

    fn detection_loader() -> Loader {
        let config = r#"
[[language]]
name = "yaml"
file-types = ["yml", "yaml"]

[[language]]
name = "docker-compose"
grammar = "yaml"
file-types = [{ glob = "docker-compose.yml" }, { glob = "docker-compose.yaml" }]

[[language]]
name = "gitignore"
file-types = [{ glob = ".*ignore" }]

[[language]]
name = "git-config"
file-types = [{ glob = ".git/config" }]
"#;
        Loader::new(
            crate::runtime::Runtime::new(&[]),
            toml::from_str(config).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn globs_take_precedence_over_extensions() {
        let loader = detection_loader();
        let lang = |name: &str| loader.language_for_name(name).unwrap();

        // The glob beats the `yml` extension, at any depth.
        assert_eq!(
            loader.language_for_filename(Path::new("docker-compose.yml")),
            Some(lang("docker-compose"))
        );
        assert_eq!(
            loader.language_for_filename(Path::new("deploy/docker-compose.yml")),
            Some(lang("docker-compose"))
        );
        // Extension fallback still works.
        assert_eq!(
            loader.language_for_filename(Path::new("ci.yml")),
            Some(lang("yaml"))
        );
        // Wildcard and path-qualified globs.
        assert_eq!(
            loader.language_for_filename(Path::new(".gitignore")),
            Some(lang("gitignore"))
        );
        assert_eq!(
            loader.language_for_filename(Path::new("repo/.git/config")),
            Some(lang("git-config"))
        );
        assert_eq!(loader.language_for_filename(Path::new("config")), None);
    }

    #[test]
    fn merge_overrides_by_name_and_appends() {
        let base = r#"
[[language]]
name = "rust"
grammar = "rust"
scope = "source.rust"
"#;
        let user = r#"
[[language]]
name = "rust"
grammar = "rust-custom"

[[language]]
name = "quipu"
grammar = "quipu"
"#;
        let merged = merge_configs(base, Some(user)).unwrap();
        let langs = merged["language"].as_array().unwrap();
        assert_eq!(langs.len(), 2, "override merges, new language appends");

        let rust = &langs[0];
        // overlay wins for overlapping keys...
        assert_eq!(rust["grammar"].as_str(), Some("rust-custom"));
        // ...but base-only keys are preserved.
        assert_eq!(rust["scope"].as_str(), Some("source.rust"));

        assert_eq!(langs[1]["name"].as_str(), Some("quipu"));
    }
}
