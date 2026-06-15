//! HTML backend: `<pre><code>` with `<span class="...">` per scope.
//!
//! A dotted scope is emitted as space-separated hierarchical classes so CSS can
//! target any level: `keyword.control.conditional` becomes
//! `class="keyword keyword-control keyword-control-conditional"`.

use super::{classes, Backend};

pub struct HtmlBackend {
    out: String,
}

impl HtmlBackend {
    pub fn new() -> Self {
        let mut out = String::from("<pre><code class=\"dathan\">");
        out.reserve(4096);
        Self { out }
    }
}

impl Backend for HtmlBackend {
    fn open(&mut self, scope: &str) {
        self.out.push_str("<span class=\"");
        self.out.push_str(&classes(scope));
        self.out.push_str("\">");
    }

    fn text(&mut self, text: &str) {
        escape_into(text, &mut self.out);
    }

    fn close(&mut self) {
        self.out.push_str("</span>");
    }

    fn finish(self: Box<Self>) -> String {
        let mut out = self.out;
        out.push_str("</code></pre>\n");
        out
    }
}

fn escape_into(text: &str, out: &mut String) {
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
    fn nested_spans_and_escaping() {
        let mut b = Box::new(HtmlBackend::new());
        b.open("keyword.control");
        b.text("if x < 1 && \"q\"");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "<pre><code class=\"dathan\">\
             <span class=\"keyword keyword-control\">if x &lt; 1 &amp;&amp; &quot;q&quot;</span>\
             </code></pre>\n"
        );
    }
}
