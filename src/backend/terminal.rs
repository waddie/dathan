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
    /// Whether the last text emission ended on a newline, so `finish` can avoid
    /// appending a second one (it only terminates the block when the content did
    /// not already end on a line boundary).
    ended_with_newline: bool,
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
            ended_with_newline: false,
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
        let before = self.out.len();
        // Fill each line's trailing cells with the active background by erasing
        // to end-of-line before every newline.
        let mut rest = text;
        while let Some(idx) = rest.find('\n') {
            let mut line = &rest[..idx];
            // For CRLF sources, the erase must precede the carriage return:
            // emitting `\x1b[K` after a `\r` would erase from column 0 and wipe
            // the line we just wrote, leaving only the background.
            let cr = line.ends_with('\r');
            if cr {
                line = &line[..line.len() - 1];
            }
            self.out.push_str(line);
            self.out.push_str(CLEAR_EOL);
            if cr {
                self.out.push('\r');
            }
            self.out.push('\n');
            rest = &rest[idx + 1..];
        }
        // A trailing `\r` with no following `\n` (e.g. a CR-only line ending, or
        // a CRLF split across spans) must not sit before the final CLEAR_EOL
        // either; strip it so `finish` fills the tail correctly.
        if let Some(stripped) = rest.strip_suffix('\r') {
            self.out.push_str(stripped);
            self.out.push_str(CLEAR_EOL);
            self.out.push('\r');
            self.line_dirty = false;
        } else {
            self.out.push_str(rest);
            self.line_dirty = !rest.is_empty();
        }
        // Record whether the content emitted in this call ended on a newline,
        // skipping no-op calls (e.g. empty text) so a prior `true` is preserved.
        // A trailing `0x0A` is unambiguous in UTF-8 (no multibyte sequence ends
        // in it).
        if self.out.len() > before {
            self.ended_with_newline = self.out.as_bytes()[self.out.len() - 1] == b'\n';
        }
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
        // newline (whose line was filled when it was emitted), avoiding an extra
        // background-coloured blank line.
        if self.line_dirty {
            out.push_str(CLEAR_EOL);
        }
        out.push_str(RESET);
        // Only terminate the block when the content did not already end on a
        // newline, so re-running on a newline-terminated source is idempotent
        // rather than adding a trailing blank line.
        if !self.ended_with_newline {
            out.push('\n');
        }
        out
    }
}

/// Flatten the open span styles into one: inner colours override outer ones,
/// modifiers accumulate.
fn cumulative(stack: &[Style]) -> Style {
    let mut out = Style::default();
    for style in stack {
        if style.fg.is_some() {
            out.fg.clone_from(&style.fg);
        }
        if style.bg.is_some() {
            out.bg.clone_from(&style.bg);
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
    fn crlf_erases_before_the_carriage_return() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.text("a\r\nb");
        let out = b.finish();
        // The erase-to-end-of-line must precede the `\r`; emitting it after the
        // carriage return would wipe the line at display time, leaving only the
        // background.
        assert_eq!(
            out,
            "\x1b[0m\x1b[38;2;204;204;204ma\x1b[K\r\nb\x1b[K\x1b[0m\n"
        );
    }

    #[test]
    fn does_not_fill_an_empty_trailing_line() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.text("a\n");
        let out = b.finish();
        // Source ended on a newline: that line was filled when emitted, no extra
        // erase is added for the empty final line, and no second newline is
        // appended (the content already ended on a line boundary).
        assert_eq!(out, "\x1b[0m\x1b[38;2;204;204;204ma\x1b[K\n\x1b[0m");
    }

    #[test]
    fn newline_ending_survives_a_trailing_close() {
        let mut b = Box::new(TerminalBackend::new(theme()));
        b.open("keyword");
        b.text("a\n");
        b.close();
        let out = b.finish();
        // The close after the final newline emits only SGR/RESET codes; it must
        // not cause `finish` to append a second newline.
        assert!(!out.ends_with("\n\x1b[0m\n"));
        assert!(out.ends_with("\x1b[0m"));
    }
}
