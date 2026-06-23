//! JSON-Hiccup backend.
//!
//! Emits hiccup as JSON arrays: `["span", {"class": "keyword keyword-control"},
//! "if", ...]`. By default the `class` value uses the same space-separated
//! hyphenated hierarchical classes as the other backends. With `--inline` the
//! [`Styler`] instead resolves each scope to a `style` string from the theme,
//! and a base style goes on the `pre`. Spans nest, so we build a stack of frames
//! and render each on `close`.

use super::{Attr, Backend, Styler};
use std::fmt::Write as _;

struct Frame {
    /// `None` for the implicit root (the `["code", ...]` children).
    scope: Option<String>,
    children: Vec<String>,
}

pub struct JsonHiccupBackend {
    stack: Vec<Frame>,
    styler: Styler,
}

impl JsonHiccupBackend {
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

impl Backend for JsonHiccupBackend {
    fn open(&mut self, scope: &str) {
        self.stack.push(Frame {
            scope: Some(scope.to_string()),
            children: Vec::new(),
        });
    }

    fn text(&mut self, text: &str) {
        let literal = json_string(text);
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
        let mut code = String::from("[\"code\",{}");
        for child in &root.children {
            code.push(',');
            code.push_str(child);
        }
        code.push(']');
        let pre_attr = self
            .styler
            .base_attr()
            .map_or_else(|| "{}".to_string(), |attr| attr_object(&attr));
        format!("[\"div\",{{\"class\":\"dathan\"}},[\"pre\",{pre_attr},{code}]]\n")
    }
}

fn render(styler: &Styler, frame: &Frame) -> String {
    let scope = frame.scope.as_deref().unwrap_or_default();
    let mut out = String::from("[\"span\"");
    if let Some(attr) = styler.span_attr(scope) {
        out.push(',');
        out.push_str(&attr_object(&attr));
    }
    for child in &frame.children {
        out.push(',');
        out.push_str(child);
    }
    out.push(']');
    out
}

/// A JSON attribute object like `{"class":"…"}`.
fn attr_object(attr: &Attr) -> String {
    format!(
        "{{{}:{}}}",
        json_string(attr.name),
        json_string(&attr.value)
    )
}

/// Render a Rust string as a JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
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
    fn nested_json_hiccup_and_escaping() {
        let mut b = Box::new(JsonHiccupBackend::new(Styler::Classes));
        b.open("keyword.function");
        b.text("fn");
        b.close();
        b.text("\t\"x\"\n");
        let out = b.finish();
        assert_eq!(
            out,
            "[\"div\",{\"class\":\"dathan\"},[\"pre\",{},[\"code\",{},\
             [\"span\",{\"class\":\"keyword keyword-function\"},\"fn\"],\
             \"\\t\\\"x\\\"\\n\"]]]\n"
        );
    }

    #[test]
    fn inline_styles_and_base() {
        let mut b = Box::new(JsonHiccupBackend::new(Styler::Inline(inline_theme())));
        b.open("keyword.function");
        b.text("fn");
        b.close();
        b.open("variable"); // unstyled -> no attr object
        b.text("x");
        b.close();
        let out = b.finish();
        assert_eq!(
            out,
            "[\"div\",{\"class\":\"dathan\"},[\"pre\",{\"style\":\"color: #cccccc\"},[\"code\",{},\
             [\"span\",{\"style\":\"color: #ff0000\"},\"fn\"],\
             [\"span\",\"x\"]]]]\n"
        );
    }
}
