//! Pluggable output backends.

mod hiccup;
mod html;

pub use hiccup::EdnHiccupBackend;
pub use html::HtmlBackend;

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
