//! HTML backend: `<pre><code>` with a `<span>` per scope.
//!
//! By default each span carries hierarchical `class`es so CSS can target any
//! level: `keyword.control.conditional` becomes
//! `class="keyword keyword-control keyword-control-conditional"`. With `--inline`
//! the [`Styler`] instead resolves each scope to a `style="…"` attribute from
//! the theme, and a base style goes on the `<pre>`.

use super::{Attr, Backend, Styler, escape_html_into};

pub struct HtmlBackend {
    out: String,
    styler: Styler,
}

impl HtmlBackend {
    pub fn new(styler: Styler) -> Self {
        let mut out = String::from("<div class=\"dathan\"><pre");
        if let Some(attr) = styler.base_attr() {
            push_attr(&mut out, &attr);
        }
        out.push_str("><code>");
        out.reserve(4096);
        Self { out, styler }
    }
}

impl Backend for HtmlBackend {
    fn open(&mut self, scope: &str) {
        self.out.push_str("<span");
        if let Some(attr) = self.styler.span_attr(scope) {
            push_attr(&mut self.out, &attr);
        }
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

/// Append ` name="value"`. Attribute values (class lists, style declarations)
/// don't contain `"`, so no escaping is needed.
fn push_attr(out: &mut String, attr: &Attr) {
    out.push(' ');
    out.push_str(attr.name);
    out.push_str("=\"");
    out.push_str(&attr.value);
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn inline_theme() -> Theme {
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
    fn nested_spans_and_escaping() {
        let mut b = Box::new(HtmlBackend::new(Styler::Classes));
        b.open("keyword.control");
        b.text("if x < 1 && \"q\"");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "<div class=\"dathan\"><pre><code>\
             <span class=\"keyword keyword-control\">if x &lt; 1 &amp;&amp; &quot;q&quot;</span>\
             </code></pre></div>\n"
        );
    }

    #[test]
    fn inline_styles_and_base() {
        let mut b = Box::new(HtmlBackend::new(Styler::Inline(inline_theme())));
        b.open("keyword.control");
        b.text("if");
        b.close();
        b.open("variable"); // unstyled -> bare span
        b.text("x");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "<div class=\"dathan\"><pre style=\"color: #cccccc\"><code>\
             <span style=\"color: #ff0000; font-weight: bold\">if</span>\
             <span>x</span>\
             </code></pre></div>\n"
        );
    }
}
