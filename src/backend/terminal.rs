//! Terminal (ANSI escape codes) backend.
//!
//! Resolves each scope to a [`Style`] through the theme and emits SGR sequences.
//! Because SGR state is flat (not nested like markup), we keep a stack of the
//! styles for the currently open spans and, on every open/close, reset and
//! re-emit the cumulative style (outer styles overlaid by inner ones). The
//! bottom of the stack is a base style taken from the theme's `ui.text` /
//! `ui.background`, so the whole block is themed.

use super::Backend;
use crate::style::Style;
use crate::theme::Theme;

const RESET: &str = "\x1b[0m";

pub struct TerminalBackend {
    out: String,
    theme: Theme,
    /// Styles for open spans; `stack[0]` is the never-popped base style.
    stack: Vec<Style>,
}

impl TerminalBackend {
    pub fn new(theme: Theme) -> Self {
        let base = Style {
            fg: theme.resolve("ui.text").fg,
            bg: theme.resolve("ui.background").bg,
            modifiers: Vec::new(),
        };
        let mut backend = Self {
            out: String::new(),
            theme,
            stack: vec![base],
        };
        backend.emit_current();
        backend
    }

    /// Reset and re-apply the cumulative style of the open span stack.
    fn emit_current(&mut self) {
        self.out.push_str(RESET);
        let params = cumulative(&self.stack).sgr_params();
        if !params.is_empty() {
            self.out.push_str("\x1b[");
            self.out.push_str(&params.join(";"));
            self.out.push('m');
        }
    }
}

impl Backend for TerminalBackend {
    fn open(&mut self, scope: &str) {
        self.stack.push(self.theme.resolve(scope));
        self.emit_current();
    }

    fn text(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn close(&mut self) {
        // Never pop the base style at index 0.
        if self.stack.len() > 1 {
            self.stack.pop();
        }
        self.emit_current();
    }

    fn finish(self: Box<Self>) -> String {
        let mut out = self.out;
        out.push_str(RESET);
        out.push('\n');
        out
    }
}

/// Flatten the open span styles into one: inner colours override outer ones,
/// modifiers accumulate.
fn cumulative(stack: &[Style]) -> Style {
    let mut out = Style::default();
    for style in stack {
        if style.fg.is_some() {
            out.fg = style.fg.clone();
        }
        if style.bg.is_some() {
            out.bg = style.bg.clone();
        }
        for &m in &style.modifiers {
            out.push_modifier(m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        let table = toml::from_str::<toml::Value>(
            r##"
"ui.text" = "#cccccc"
keyword = { fg = "#ff0000", modifiers = ["bold"] }
"##,
        )
        .unwrap();
        Theme::from_table(table.as_table().unwrap())
    }

    #[test]
    fn resets_and_reapplies_around_spans() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.open("keyword");
        b.text("fn");
        b.close();
        b.text(" x");
        let out = b.finish();
        assert_eq!(
            out,
            // base (ui.text) -> open keyword (red bold) -> close back to base -> reset
            "\x1b[0m\x1b[38;2;204;204;204m\
             \x1b[0m\x1b[38;2;255;0;0;1mfn\
             \x1b[0m\x1b[38;2;204;204;204m x\
             \x1b[0m\n"
        );
    }
}
