//! Clojure/EDN Hiccup backend.
//!
//! Emits `[:pre [:code [:span {:class "keyword keyword-control"} "if"] " " ...]]`.
//! The `:class` value uses the same space-separated hyphenated hierarchical
//! classes as the HTML backend, so naming is consistent across formats. Spans
//! nest, so we build a stack of frames and render each on `close`.

use super::{classes, Backend};

struct Frame {
    /// `None` for the implicit root (the `[:code ...]` children).
    class: Option<String>,
    children: Vec<String>,
}

pub struct EdnHiccupBackend {
    stack: Vec<Frame>,
}

impl EdnHiccupBackend {
    pub fn new() -> Self {
        Self {
            stack: vec![Frame {
                class: None,
                children: Vec::new(),
            }],
        }
    }
}

impl Backend for EdnHiccupBackend {
    fn open(&mut self, scope: &str) {
        self.stack.push(Frame {
            class: Some(scope.to_string()),
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
        let rendered = render(&frame);
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(rendered);
        }
    }

    fn finish(self: Box<Self>) -> String {
        let root = &self.stack[0];
        let mut out = String::from("[:pre [:code");
        for child in &root.children {
            out.push(' ');
            out.push_str(child);
        }
        out.push_str("]]\n");
        out
    }
}

fn render(frame: &Frame) -> String {
    let scope = frame.class.as_deref().unwrap_or_default();
    let mut out = format!("[:span {{:class {}}}", edn_string(&classes(scope)));
    for child in &frame.children {
        out.push(' ');
        out.push_str(child);
    }
    out.push(']');
    out
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

    #[test]
    fn nested_hiccup_and_edn_escaping() {
        let mut b = Box::new(EdnHiccupBackend::new());
        b.open("keyword.function");
        b.text("fn");
        b.close();
        b.text("\t\"x\"\n");
        let out = b.finish();
        assert_eq!(
            out,
            "[:pre [:code [:span {:class \"keyword keyword-function\"} \"fn\"] \"\\t\\\"x\\\"\\n\"]]\n"
        );
    }
}
