//! Clojure/EDN Hiccup backend.
//!
//! Emits `[:pre [:code [:span {:class "keyword keyword-control"} "if"] " " ...]]`.
//! By default the `:class` value uses the same space-separated hyphenated
//! hierarchical classes as the other backends. With `--inline` the [`Styler`]
//! instead resolves each scope to a `:style` string from the theme, and a base
//! style goes on the `:pre`. Spans nest, so we build a stack of frames and
//! render each on `close`.

use super::{Attr, Backend, Styler};

struct Frame {
    /// `None` for the implicit root (the `[:code ...]` children).
    scope: Option<String>,
    children: Vec<String>,
}

pub struct EdnHiccupBackend {
    stack: Vec<Frame>,
    styler: Styler,
}

impl EdnHiccupBackend {
    pub fn new(styler: Styler) -> Self {
        Self {
            stack: vec![Frame {
                scope: None,
                children: Vec::new(),
            }],
            styler,
        }
    }
}

impl Backend for EdnHiccupBackend {
    fn open(&mut self, scope: &str) {
        self.stack.push(Frame {
            scope: Some(scope.to_string()),
            children: Vec::new(),
        });
    }

    fn text(&mut self, text: &str) {
        // The driver coalesces runs of equal scope, but tree-sitter can still
        // split text at injection/event boundaries; merge adjacent literals.
        let literal = edn_string(text);
        if let Some(frame) = self.stack.last_mut() {
            frame.children.push(literal);
        }
    }

    fn close(&mut self) {
        let frame = self.stack.pop().expect("close without matching open");
        let rendered = render(&self.styler, &frame);
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(rendered);
        }
    }

    fn finish(self: Box<Self>) -> String {
        let root = &self.stack[0];
        let mut out = String::from("[:div.dathan [:pre");
        if let Some(attr) = self.styler.base_attr() {
            out.push(' ');
            out.push_str(&attr_map(&attr));
        }
        out.push_str(" [:code");
        for child in &root.children {
            out.push(' ');
            out.push_str(child);
        }
        out.push_str("]]]\n");
        out
    }
}

fn render(styler: &Styler, frame: &Frame) -> String {
    let scope = frame.scope.as_deref().unwrap_or_default();
    let mut out = String::from("[:span");
    if let Some(attr) = styler.span_attr(scope) {
        out.push(' ');
        out.push_str(&attr_map(&attr));
    }
    for child in &frame.children {
        out.push(' ');
        out.push_str(child);
    }
    out.push(']');
    out
}

/// An EDN attribute map like `{:class "…"}`.
fn attr_map(attr: &Attr) -> String {
    format!("{{:{} {}}}", attr.name, edn_string(&attr.value))
}

/// Render a Rust string as an EDN string literal.
fn edn_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn inline_theme() -> Theme {
        let table = toml::from_str::<toml::Value>(
            r##"
"ui.text" = "#cccccc"
"keyword.function" = "#ff0000"
"##,
        )
        .unwrap();
        Theme::from_table(table.as_table().unwrap())
    }

    #[test]
    fn nested_hiccup_and_edn_escaping() {
        let mut b = Box::new(EdnHiccupBackend::new(Styler::Classes));
        b.open("keyword.function");
        b.text("fn");
        b.close();
        b.text("\t\"x\"\n");
        let out = b.finish();
        assert_eq!(
            out,
            "[:div.dathan [:pre [:code [:span {:class \"keyword keyword-function\"} \"fn\"] \"\\t\\\"x\\\"\\n\"]]]\n"
        );
    }

    #[test]
    fn inline_styles_and_base() {
        let mut b = Box::new(EdnHiccupBackend::new(Styler::Inline(inline_theme())));
        b.open("keyword.function");
        b.text("fn");
        b.close();
        b.open("variable"); // unstyled -> no attr map
        b.text("x");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "[:div.dathan [:pre {:style \"color: #cccccc\"} [:code [:span {:style \"color: #ff0000\"} \"fn\"] [:span \"x\"]]]]\n"
        );
    }
}
