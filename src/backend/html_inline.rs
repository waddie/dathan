//! HTML backend with inline styles.
//!
//! Like the class-based HTML backend, but resolves each scope to a [`Style`]
//! through the theme and emits `style="color:…"` per span instead of relying on
//! an external stylesheet. Spans nest in HTML, so each span carries only its own
//! resolved style. The `ui.text` / `ui.background` styles, if present, go on the
//! `<pre>` element as a base.

use super::{Backend, escape_html_into};
use crate::style::Style;
use crate::theme::Theme;

pub struct HtmlInlineBackend {
    out: String,
    theme: Theme,
}

impl HtmlInlineBackend {
    pub fn new(theme: Theme) -> Self {
        let base = Style {
            fg: theme.resolve("ui.text").fg,
            bg: theme.resolve("ui.background").bg,
            modifiers: Vec::new(),
        };
        let mut out = String::from("<div class=\"dathan\"><pre");
        out.push_str(&style_attr(&base));
        out.push_str("><code>");
        out.reserve(4096);
        Self { out, theme }
    }
}

impl Backend for HtmlInlineBackend {
    fn open(&mut self, scope: &str) {
        let style = self.theme.resolve(scope);
        self.out.push_str("<span");
        self.out.push_str(&style_attr(&style));
        self.out.push('>');
    }

    fn text(&mut self, text: &str) {
        escape_html_into(text, &mut self.out);
    }

    fn close(&mut self) {
        self.out.push_str("</span>");
    }

    fn finish(self: Box<Self>) -> String {
        let mut out = self.out;
        out.push_str("</code></pre></div>\n");
        out
    }
}

/// A ` style="…"` attribute for the style, or the empty string if it has no
/// declarations.
fn style_attr(style: &Style) -> String {
    let decls = style.css_declarations();
    if decls.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", decls.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        let table = toml::from_str::<toml::Value>(
            r##"
"ui.text" = "#cccccc"
"keyword.control" = { fg = "#ff0000", modifiers = ["bold"] }
"##,
        )
        .unwrap();
        Theme::from_table(table.as_table().unwrap())
    }

    #[test]
    fn inline_styles_and_escaping() {
        let mut b = Box::new(HtmlInlineBackend::new(theme()));
        b.open("keyword.control");
        b.text("if x < 1");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "<div class=\"dathan\"><pre style=\"color: #cccccc\"><code>\
             <span style=\"color: #ff0000; font-weight: bold\">if x &lt; 1</span>\
             </code></pre></div>\n"
        );
    }

    #[test]
    fn unstyled_scope_emits_bare_span() {
        let mut b = Box::new(HtmlInlineBackend::new(theme()));
        b.open("variable");
        b.text("x");
        b.close();
        let out = b.finish();
        assert!(out.contains("<span>x</span>"));
    }
}
