//! Highlight driver.
//!
//! Drives `tree_house`'s `Highlighter`, which yields, at each event offset, the
//! full stack of active highlights (outermost first) for the following text
//! span. We diff successive stacks into nested open/close span events and feed
//! them to a `Backend`, emitting properly nested markup.

use std::time::Duration;

use anyhow::{anyhow, Result};
use ropey::RopeSlice;
use tree_house::highlighter::{Highlight, HighlightEvent, Highlighter};
use tree_house::{Language, Syntax};

use crate::backend::Backend;
use crate::languages::Loader;

const PARSE_TIMEOUT: Duration = Duration::from_secs(15);

/// Highlight `source` as `lang`, driving `backend` with open/text/close calls.
pub fn highlight(
    loader: &Loader,
    lang: Language,
    source: &str,
    backend: &mut dyn Backend,
) -> Result<()> {
    let rope = RopeSlice::from(source);
    let len = source.len() as u32;

    let syntax = Syntax::new(rope, lang, PARSE_TIMEOUT, loader)
        .map_err(|e| anyhow!("failed to parse source: {e:?}"))?;
    let mut highlighter = Highlighter::new(&syntax, rope, loader, ..);

    // Mirror of the highlighter's active stack (outermost first).
    let mut stack: Vec<Highlight> = Vec::new();
    // Spans currently open in the backend output.
    let mut open: Vec<Highlight> = Vec::new();

    let mut pos = 0u32;
    while pos < len {
        if pos == highlighter.next_event_offset() {
            let (event, new_highlights) = highlighter.advance();
            if event == HighlightEvent::Refresh {
                stack.clear();
            }
            stack.extend(new_highlights);
        }

        let start = pos;
        let next = highlighter.next_event_offset();
        pos = if next == u32::MAX || next > len {
            len
        } else {
            next
        };

        if pos <= start {
            if pos >= len {
                break;
            }
            // Zero-width region: loop to drain further events at this offset.
            continue;
        }

        sync_spans(&mut open, &stack, loader, backend);
        backend.text(&source[start as usize..pos as usize]);
    }

    while open.pop().is_some() {
        backend.close();
    }
    Ok(())
}

/// Reconcile the open spans with the desired stack: keep the common prefix,
/// close the rest, then open the new tail.
fn sync_spans(
    open: &mut Vec<Highlight>,
    stack: &[Highlight],
    loader: &Loader,
    backend: &mut dyn Backend,
) {
    let common = open
        .iter()
        .zip(stack.iter())
        .take_while(|(a, b)| a == b)
        .count();

    for _ in common..open.len() {
        backend.close();
    }
    open.truncate(common);

    for &highlight in &stack[common..] {
        backend.open(&loader.scope_name(highlight));
        open.push(highlight);
    }
}
