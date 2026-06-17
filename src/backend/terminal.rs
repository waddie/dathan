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
/// Erase-in-Line (to end): paints from the cursor to the right edge with the
/// current background, so the theme background reaches the full terminal width
/// rather than stopping at the last character of each line.
const CLEAR_EOL: &str = "\x1b[K";

pub struct TerminalBackend {
    out: String,
    theme: Theme,
    /// Styles for open spans; `stack[0]` is the never-popped base style.
    stack: Vec<Style>,
    /// Whether the current line has content past the last emitted `CLEAR_EOL`,
    /// so `finish` knows to fill the final line's tail.
    line_dirty: bool,
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
            line_dirty: false,
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
        // Fill each line's trailing cells with the active background by erasing
        // to end-of-line before every newline.
        let mut rest = text;
        while let Some(idx) = rest.find('\n') {
            self.out.push_str(&rest[..idx]);
            self.out.push_str(CLEAR_EOL);
            self.out.push('\n');
            rest = &rest[idx + 1..];
        }
        self.out.push_str(rest);
        self.line_dirty = !rest.is_empty();
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
        // Fill the final line's tail too, unless the source already ended on a
        // newline (whose line was filled when it was emitted) — avoids an extra
        // background-coloured blank line.
        if self.line_dirty {
            out.push_str(CLEAR_EOL);
        }
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
            // base (ui.text) -> open keyword (red bold) -> close back to base ->
            // fill the final line's tail -> reset
            "\x1b[0m\x1b[38;2;204;204;204m\
             \x1b[0m\x1b[38;2;255;0;0;1mfn\
             \x1b[0m\x1b[38;2;204;204;204m x\
             \x1b[K\x1b[0m\n"
        );
    }

    #[test]
    fn fills_each_line_to_the_edge() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.text("a\nb");
        let out = b.finish();
        // Each newline is preceded by an erase-to-end-of-line, and the last
        // line (no trailing newline) is filled before the reset.
        assert_eq!(
            out,
            "\x1b[0m\x1b[38;2;204;204;204ma\x1b[K\nb\x1b[K\x1b[0m\n"
        );
    }

    #[test]
    fn does_not_fill_an_empty_trailing_line() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.text("a\n");
        let out = b.finish();
        // Source ended on a newline: that line was filled when emitted, and no
        // extra erase is added for the empty final line.
        assert_eq!(out, "\x1b[0m\x1b[38;2;204;204;204ma\x1b[K\n\x1b[0m\n");
    }
}
