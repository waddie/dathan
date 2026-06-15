//! Query loading with `; inherits:` resolution.
//!
//! `tree_house::read_query` does the recursive in-place `; inherits: a,b`
//! expansion; we just supply a reader that pulls each language's query file
//! from the runtime search path.

use std::path::PathBuf;

use crate::runtime::Runtime;

/// Read `queries/<lang>/<filename>` with inherits expanded. Missing files
/// resolve to an empty string, matching Helix.
pub fn read_query(rt: &Runtime, lang: &str, filename: &str) -> String {
    tree_house::read_query(lang, |language| {
        let rel = PathBuf::from("queries").join(language).join(filename);
        rt.find_file(&rel)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    })
}
