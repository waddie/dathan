//! dathan — highlight source code to HTML or Clojure/EDN Hiccup using Helix's
//! tree-sitter grammars and queries.

mod backend;
mod grammar;
mod highlight;
mod languages;
mod queries;
mod runtime;
mod theme;

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};

use backend::{Backend, EdnHiccupBackend, HtmlBackend};
use languages::Loader;
use runtime::{home_dir, Runtime};

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Html,
    EdnHiccup,
}

#[derive(Parser)]
#[command(
    name = "dathan",
    about = "Highlight code to HTML or Clojure/EDN Hiccup via Helix grammars"
)]
struct Cli {
    /// Source file to highlight (reads from stdin if omitted).
    file: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value = "edn-hiccup")]
    format: Format,

    /// Override detected language by name (e.g. `rust`).
    #[arg(long)]
    lang: Option<String>,

    /// Extra runtime root(s); highest priority. May be repeated.
    #[arg(long)]
    runtime: Vec<PathBuf>,

    /// Path to a languages.toml (defaults to the Helix source/user config).
    #[arg(long)]
    languages: Option<PathBuf>,

    /// theme.toml to use for --emit-css.
    #[arg(long)]
    theme: Option<PathBuf>,

    /// Emit a CSS stylesheet from the theme and exit (ignores FILE).
    #[arg(long)]
    emit_css: bool,

    /// Output file (default: stdout).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let rt = Runtime::new(&cli.runtime);

    if cli.emit_css {
        let theme_path = resolve_theme(&cli, &rt)?;
        let theme_toml = std::fs::read_to_string(&theme_path)
            .with_context(|| format!("reading theme {}", theme_path.display()))?;
        let css = theme::to_css(&theme_toml)?;
        return write_output(cli.output.as_deref(), &css);
    }

    let file = cli.file.as_deref();
    let source = match file {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading from stdin")?;
            buf
        }
    };

    let merged = load_languages(&cli)?;
    let loader = Loader::new(rt, merged)?;

    let lang = detect_language(&loader, &cli, file, &source).ok_or_else(|| match file {
        Some(path) => anyhow!(
            "could not determine language for {} (try --lang)",
            path.display()
        ),
        None => anyhow!("could not determine language for stdin (try --lang)"),
    })?;

    let mut backend: Box<dyn Backend> = match cli.format {
        Format::Html => Box::new(HtmlBackend::new()),
        Format::EdnHiccup => Box::new(EdnHiccupBackend::new()),
    };
    highlight::highlight(&loader, lang, &source, backend.as_mut())?;
    let rendered = backend.finish();

    write_output(cli.output.as_deref(), &rendered)
}

fn detect_language(
    loader: &Loader,
    cli: &Cli,
    file: Option<&Path>,
    source: &str,
) -> Option<tree_house::Language> {
    if let Some(name) = &cli.lang {
        return loader.language_for_name(name);
    }
    file.and_then(|f| loader.language_for_filename(f))
        .or_else(|| {
            let first_line = source.lines().next().unwrap_or_default();
            loader.language_for_shebang(first_line)
        })
}

/// Load the base `languages.toml` and merge the user config over it (by
/// language name), mirroring how Helix layers user config on the default.
fn load_languages(cli: &Cli) -> Result<toml::Value> {
    let user_path = home_dir().map(|h| h.join(".config/helix/languages.toml"));

    let base_path = cli
        .languages
        .clone()
        .or_else(|| {
            first_existing([
                home_dir().map(|h| h.join("source/helix/languages.toml")),
                user_path.clone(),
            ])
        })
        .ok_or_else(|| anyhow!("no languages.toml found; pass --languages <path>"))?;

    let base = std::fs::read_to_string(&base_path)
        .with_context(|| format!("reading {}", base_path.display()))?;

    // Overlay the user config unless it is already the base we loaded.
    let overlay = match user_path {
        Some(path) if path.exists() && path != base_path => Some(
            std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?,
        ),
        _ => None,
    };

    languages::merge_configs(&base, overlay.as_deref())
}

fn resolve_theme(cli: &Cli, rt: &Runtime) -> Result<PathBuf> {
    if let Some(path) = &cli.theme {
        return Ok(path.clone());
    }
    let mut candidates = vec![home_dir().map(|h| h.join("source/helix/theme.toml"))];
    // A theme bundled alongside any runtime root's parent.
    for root in rt.roots() {
        candidates.push(root.parent().map(|p| p.join("theme.toml")));
    }
    first_existing(candidates).ok_or_else(|| anyhow!("no theme.toml found; pass --theme <path>"))
}

fn first_existing<I>(candidates: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = Option<PathBuf>>,
{
    candidates.into_iter().flatten().find(|p| p.exists())
}

fn write_output(output: Option<&Path>, content: &str) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))
        }
        None => {
            print!("{content}");
            Ok(())
        }
    }
}
