//! Helix `theme.toml` loading: a [`Theme`] of resolved [`Style`]s, used both to
//! emit a CSS stylesheet and to drive the theme-aware backends.
//!
//! Each scope key becomes a rule whose selector matches the most specific class
//! the HTML backend emits: `keyword.control` -> `.keyword-control`. Palette
//! names are resolved via the theme's `[palette]` table.
//!
//! Themes may inherit from a parent via `inherits = "<name>"`. The parent is
//! located by name in the theme search directories (or, for `default` /
//! `base16_default`, from data bundled with dathan) and merged following Helix's
//! rules: palette entries override per-key, while a scope defined in the child
//! fully replaces the parent's.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use toml::Value;
use toml::value::Table;

use crate::style::{Modifier, Style};

/// Helix's built-in `default` theme, bundled so themes that `inherits = "default"`
/// resolve without a runtime file.
const DEFAULT_THEME: &str = include_str!("../assets/default.toml");
/// Helix's built-in `base16_default` theme.
const BASE16_DEFAULT_THEME: &str = include_str!("../assets/base16_default.toml");

/// A loaded theme: dotted scope -> resolved [`Style`], with palette names and
/// inheritance already resolved away.
pub struct Theme {
    scopes: HashMap<String, Style>,
}

impl Theme {
    /// Load a theme from `path`, resolving `inherits` against `theme_dirs`.
    pub fn load(path: &Path, theme_dirs: &[PathBuf]) -> Result<Self> {
        let mut visited = HashSet::new();
        visited.insert(path.to_path_buf());
        let toml = std::fs::read_to_string(path)
            .with_context(|| format!("reading theme {}", path.display()))?;
        let value: Value =
            toml::from_str(&toml).with_context(|| format!("parsing theme {}", path.display()))?;
        let merged = resolve(value, theme_dirs, &mut visited)?;
        let table = merged.as_table().context("theme is not a table")?.clone();
        Ok(Self::from_table(&table))
    }

    /// Build from the bundled `default` theme, used when no theme is configured.
    pub fn bundled_default() -> Self {
        let value: Value = toml::from_str(DEFAULT_THEME).expect("bundled default theme parses");
        let table = value
            .as_table()
            .expect("bundled default theme is a table")
            .clone();
        Self::from_table(&table)
    }

    pub(crate) fn from_table(table: &Table) -> Self {
        let palette = palette_map(table);
        let resolve_color = |c: &str| palette.get(c).cloned().unwrap_or_else(|| c.to_string());

        let mut scopes = HashMap::new();
        for (key, value) in table {
            if key == "palette" || key == "inherits" {
                continue;
            }
            if let Some(style) = parse_style(value, &resolve_color) {
                scopes.insert(key.clone(), style);
            }
        }
        Self { scopes }
    }

    /// Resolve a dotted scope to a style using longest-prefix matching, the same
    /// fallback Helix uses: `function.builtin` tries `function.builtin`, then
    /// `function`.
    pub fn resolve(&self, scope: &str) -> Style {
        let mut name = scope;
        loop {
            if let Some(style) = self.scopes.get(name) {
                return style.clone();
            }
            match name.rfind('.') {
                Some(i) => name = &name[..i],
                None => return Style::default(),
            }
        }
    }

    /// Render the theme to a CSS stylesheet, one rule per scope keyed on the most
    /// specific class the HTML backend emits.
    pub fn to_css(&self) -> String {
        let mut css = String::from("");
        css.push_str(".dathan {\n");
        // Container colours, mirroring the base attribute the inline HTML backend
        // sets on `<pre>` from `ui.text` / `ui.background`.
        let base = Style {
            fg: self.resolve("ui.text").fg,
            bg: self.resolve("ui.background").bg,
            modifiers: Vec::new(),
        };
        let base_decls = base.css_declarations();
        if !base_decls.is_empty() {
            css.push_str(&format!("  & > pre {{ {}; }}\n", base_decls.join("; ")));
        }
        let mut scopes: Vec<(&String, &Style)> = self.scopes.iter().collect();
        scopes.sort_by(|a, b| a.0.cmp(b.0));
        for (scope, style) in scopes {
            let decls = style.css_declarations();
            if decls.is_empty() {
                continue;
            }
            css.push_str(&format!(
                "  .{} {{ {}; }}\n",
                css_class(scope),
                decls.join("; ")
            ));
        }
        css.push_str("}\n");
        css
    }
}

/// Load a theme from `path`, resolving `inherits` against `theme_dirs`, and
/// render the merged result to CSS.
pub fn load_css(path: &Path, theme_dirs: &[PathBuf]) -> Result<String> {
    Ok(Theme::load(path, theme_dirs)?.to_css())
}

/// Extract the `[palette]` table as a name -> colour-string map.
fn palette_map(table: &Table) -> HashMap<String, String> {
    table
        .get("palette")
        .and_then(Value::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a theme scope value (`"color"` or `{ fg, bg, modifiers, underline }`)
/// into a [`Style`], resolving palette names. Returns `None` for values that
/// carry no style, so empty entries neither emit CSS nor shadow a less specific
/// scope during resolution.
fn parse_style(value: &Value, resolve_color: &impl Fn(&str) -> String) -> Option<Style> {
    let style = match value {
        Value::String(color) => Style {
            fg: Some(resolve_color(color)),
            bg: None,
            modifiers: Vec::new(),
        },
        Value::Table(t) => {
            let mut style = Style::default();
            if let Some(fg) = t.get("fg").and_then(Value::as_str) {
                style.fg = Some(resolve_color(fg));
            }
            if let Some(bg) = t.get("bg").and_then(Value::as_str) {
                style.bg = Some(resolve_color(bg));
            }
            if let Some(mods) = t.get("modifiers").and_then(Value::as_array) {
                for m in mods.iter().filter_map(Value::as_str) {
                    if let Some(m) = Modifier::parse(m) {
                        style.push_modifier(m);
                    }
                }
            }
            if t.get("underline").is_some() {
                style.push_modifier(Modifier::Underlined);
            }
            style
        }
        _ => return None,
    };

    if style.is_empty() { None } else { Some(style) }
}

/// Recursively merge `theme` with its parent (if it declares `inherits`).
fn resolve(theme: Value, theme_dirs: &[PathBuf], visited: &mut HashSet<PathBuf>) -> Result<Value> {
    let Some(parent_name) = theme.get("inherits") else {
        return Ok(theme);
    };
    let parent_name = parent_name
        .as_str()
        .ok_or_else(|| anyhow!("expected 'inherits' to be a string"))?
        .to_string();

    let parent = match parent_name.as_str() {
        "default" => toml::from_str(DEFAULT_THEME).context("parsing bundled default theme")?,
        "base16_default" => {
            toml::from_str(BASE16_DEFAULT_THEME).context("parsing bundled base16_default theme")?
        }
        _ => {
            let parent_path = find_theme(&parent_name, theme_dirs, visited)?;
            visited.insert(parent_path.clone());
            let toml = std::fs::read_to_string(&parent_path)
                .with_context(|| format!("reading parent theme {}", parent_path.display()))?;
            let value: Value = toml::from_str(&toml)
                .with_context(|| format!("parsing parent theme {}", parent_path.display()))?;
            resolve(value, theme_dirs, visited)?
        }
    };

    Ok(merge_themes(parent, theme))
}

/// Locate `<name>.toml` in the search directories, skipping already-visited
/// paths so inheritance cycles surface as an error rather than looping.
fn find_theme(name: &str, theme_dirs: &[PathBuf], visited: &HashSet<PathBuf>) -> Result<PathBuf> {
    let filename = format!("{name}.toml");
    let mut cycle = false;
    theme_dirs
        .iter()
        .find_map(|dir| {
            let path = dir.join(&filename);
            if !path.exists() {
                None
            } else if visited.contains(&path) {
                cycle = true;
                None
            } else {
                Some(path)
            }
        })
        .ok_or_else(|| {
            if cycle {
                anyhow!("cycle found inheriting theme: {name}")
            } else {
                anyhow!("parent theme not found: {name}")
            }
        })
}

/// Merge a child theme into its parent, following Helix's strategy: palette
/// entries merge per-key (depth 2), the rest merges at depth 1 so a scope in the
/// child fully replaces the parent's.
fn merge_themes(parent: Value, child: Value) -> Value {
    let parent_palette = parent.get("palette");
    let child_palette = child.get("palette");

    let palette_values = match (parent_palette, child_palette) {
        (Some(p), Some(c)) => merge_toml_values(p.clone(), c.clone(), 2),
        (Some(p), None) => p.clone(),
        (None, Some(c)) => c.clone(),
        (None, None) => Value::Table(Table::new()),
    };

    let mut palette = Table::new();
    palette.insert(String::from("palette"), palette_values);

    let theme = merge_toml_values(parent, child, 1);
    merge_toml_values(theme, Value::Table(palette), 1)
}

/// Recursively merge two TOML values, with `right` taking precedence. Ported
/// from Helix's `helix_loader::merge_toml_values`.
fn merge_toml_values(left: Value, right: Value, merge_depth: usize) -> Value {
    fn get_name(v: &Value) -> Option<&str> {
        v.get("name").and_then(Value::as_str)
    }

    match (left, right) {
        (Value::Array(mut left_items), Value::Array(right_items)) => {
            if merge_depth > 0 {
                left_items.reserve(right_items.len());
                for rvalue in right_items {
                    let lvalue = get_name(&rvalue)
                        .and_then(|rname| {
                            left_items.iter().position(|v| get_name(v) == Some(rname))
                        })
                        .map(|lpos| left_items.remove(lpos));
                    let mvalue = match lvalue {
                        Some(lvalue) => merge_toml_values(lvalue, rvalue, merge_depth - 1),
                        None => rvalue,
                    };
                    left_items.push(mvalue);
                }
                Value::Array(left_items)
            } else {
                Value::Array(right_items)
            }
        }
        (Value::Table(mut left_map), Value::Table(right_map)) => {
            if merge_depth > 0 {
                for (rname, rvalue) in right_map {
                    match left_map.remove(&rname) {
                        Some(lvalue) => {
                            let merged_value = merge_toml_values(lvalue, rvalue, merge_depth - 1);
                            left_map.insert(rname, merged_value);
                        }
                        None => {
                            left_map.insert(rname, rvalue);
                        }
                    }
                }
                Value::Table(left_map)
            } else {
                Value::Table(right_map)
            }
        }
        (_, value) => value,
    }
}

/// `keyword.control` -> `keyword-control` (matches the HTML backend's most
/// specific class).
fn css_class(scope: &str) -> String {
    scope.replace('.', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Value {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn palette_and_modifiers() {
        let theme = parse(
            r##"
"keyword.control" = { fg = "blue", modifiers = ["bold"] }
comment = "gray"
[palette]
blue = "#0000ff"
gray = "#808080"
"##,
        );
        let css = Theme::from_table(theme.as_table().unwrap()).to_css();
        assert!(css.contains(".keyword-control { color: #0000ff; font-weight: bold; }"));
        assert!(css.contains(".comment { color: #808080; }"));
    }

    #[test]
    fn container_colours_on_dathan_pre() {
        let theme = parse(
            r##"
"ui.text" = "fg"
"ui.background" = { bg = "bg" }
[palette]
fg = "#cccccc"
bg = "#111111"
"##,
        );
        let css = Theme::from_table(theme.as_table().unwrap()).to_css();
        assert!(css.contains(".dathan {"));
        assert!(css.contains("& > pre { color: #cccccc; background-color: #111111; }"));
    }

    #[test]
    fn palette_overrides_per_key() {
        let parent = parse(
            r##"
keyword = { fg = "blue" }
comment = { fg = "gray" }
[palette]
blue = "#0000ff"
gray = "#808080"
"##,
        );
        let child = parse(
            r##"
inherits = "parent"
[palette]
blue = "#1111ff"
"##,
        );
        let merged = merge_themes(parent, child);
        let css = Theme::from_table(merged.as_table().unwrap()).to_css();
        // child overrode `blue`...
        assert!(css.contains(".keyword { color: #1111ff; }"));
        // ...but the parent's `gray` is retained.
        assert!(css.contains(".comment { color: #808080; }"));
    }

    #[test]
    fn child_scope_replaces_parent_and_parent_only_kept() {
        let parent = parse(
            r##"
keyword = { fg = "#000000", modifiers = ["bold"] }
comment = { fg = "#808080" }
"##,
        );
        let child = parse(
            r##"
inherits = "parent"
keyword = { fg = "#ff0000" }
"##,
        );
        let merged = merge_themes(parent, child);
        let css = Theme::from_table(merged.as_table().unwrap()).to_css();
        // child's keyword fully replaces parent's (no leftover bold).
        assert!(css.contains(".keyword { color: #ff0000; }"));
        assert!(!css.contains("font-weight: bold"));
        // parent-only scope retained.
        assert!(css.contains(".comment { color: #808080; }"));
    }

    #[test]
    fn cycle_is_detected() {
        let dir = std::env::temp_dir().join(format!("dathan-cycle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.toml"), "inherits = \"b\"\n").unwrap();
        std::fs::write(dir.join("b.toml"), "inherits = \"a\"\n").unwrap();
        let err = load_css(&dir.join("a.toml"), std::slice::from_ref(&dir)).unwrap_err();
        assert!(err.to_string().contains("cycle"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_uses_longest_prefix() {
        let theme = parse(
            r##"
function = "#000001"
"function.builtin" = "#000002"
"##,
        );
        let theme = Theme::from_table(theme.as_table().unwrap());
        // exact match
        assert_eq!(
            theme.resolve("function.builtin").fg.as_deref(),
            Some("#000002")
        );
        // falls back to the less specific scope
        assert_eq!(
            theme.resolve("function.method").fg.as_deref(),
            Some("#000001")
        );
        // no match at all -> empty style
        assert!(theme.resolve("variable").is_empty());
    }
}
