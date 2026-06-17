//! Pluggable output backends.

mod hiccup;
mod html;
mod json_hiccup;
mod terminal;

pub use hiccup::EdnHiccupBackend;
pub use html::HtmlBackend;
pub use json_hiccup::JsonHiccupBackend;
pub use terminal::TerminalBackend;

use crate::style::Style;
use crate::theme::Theme;

/// A streaming sink for highlight events. `open`/`close` bracket a highlighted
/// span (spans nest); `text` receives a raw source slice the backend must
/// escape for its format.
pub trait Backend {
    /// Open a span for the given dotted scope (e.g. `keyword.control`).
    fn open(&mut self, scope: &str);
    /// Append a raw (unescaped) source slice.
    fn text(&mut self, text: &str);
    /// Close the most recently opened span.
    fn close(&mut self);
    /// Consume the backend and produce the finished document.
    fn finish(self: Box<Self>) -> String;
}

/// A markup attribute (name plus value) for a span or the container element.
pub(crate) struct Attr {
    pub name: &'static str,
    pub value: String,
}

/// How the class-based backends (HTML, EDN/JSON Hiccup) turn a scope into a
/// span attribute: either hierarchical `class`es, or inline `style`s resolved
/// from a theme (selected with `--inline`).
pub(crate) enum Styler {
    Classes,
    Inline(Theme),
}

impl Styler {
    /// The attribute for a span with the given scope, or `None` when there is
    /// nothing to emit (inline mode with an unstyled scope).
    pub(crate) fn span_attr(&self, scope: &str) -> Option<Attr> {
        match self {
            Styler::Classes => Some(Attr {
                name: "class",
                value: classes(scope),
            }),
            Styler::Inline(theme) => inline_attr(theme.resolve(scope)),
        }
    }

    /// The base attribute for the container element, or `None`. Only inline mode
    /// sets one, from the theme's `ui.text` / `ui.background`.
    pub(crate) fn base_attr(&self) -> Option<Attr> {
        match self {
            Styler::Classes => None,
            Styler::Inline(theme) => inline_attr(Style {
                fg: theme.resolve("ui.text").fg,
                bg: theme.resolve("ui.background").bg,
                modifiers: Vec::new(),
            }),
        }
    }
}

/// A `style` attribute for the resolved style, or `None` if it has no
/// declarations.
fn inline_attr(style: Style) -> Option<Attr> {
    let decls = style.css_declarations();
    if decls.is_empty() {
        None
    } else {
        Some(Attr {
            name: "style",
            value: decls.join("; "),
        })
    }
}

/// Convert a dotted scope into space-separated hierarchical classes, shared by
/// every backend (and matched by the CSS emitter) so naming is consistent:
/// `keyword.control.conditional` ->
/// `keyword keyword-control keyword-control-conditional`.
pub(crate) fn classes(scope: &str) -> String {
    let parts: Vec<&str> = scope.split('.').collect();
    let mut classes = Vec::with_capacity(parts.len());
    for i in 1..=parts.len() {
        classes.push(parts[..i].join("-"));
    }
    classes.join(" ")
}

/// Append `text` to `out`, escaping the characters that are unsafe in HTML text
/// and double-quoted attribute values.
pub(crate) fn escape_html_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchical_classes() {
        assert_eq!(classes("keyword"), "keyword");
        assert_eq!(
            classes("keyword.control.conditional"),
            "keyword keyword-control keyword-control-conditional"
        );
    }
}
