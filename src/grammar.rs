//! Grammar loading.
//!
//! Thin wrapper over `tree_house::tree_sitter::Grammar::new`, which dlopens the
//! precompiled shared library, resolves the `tree_sitter_<name>` symbol, leaks
//! the library so the grammar outlives it, and verifies the ABI version
//! (13..=15) — exactly how Helix loads grammars.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use tree_house::tree_sitter::Grammar;

use crate::runtime::{Runtime, DYLIB_EXT};

/// Load the grammar named `name` from the first runtime root that provides it.
pub fn load(name: &str, rt: &Runtime) -> Result<Grammar> {
    let mut rel = PathBuf::from("grammars");
    rel.push(name);
    rel.set_extension(DYLIB_EXT);

    let path = rt.find_file(&rel).ok_or_else(|| {
        anyhow!("no compiled grammar for '{name}' (looked for grammars/{name}.{DYLIB_EXT})")
    })?;

    // SAFETY: the file is a Helix-compiled tree-sitter grammar exporting the
    // expected `tree_sitter_<name>` constructor; `Grammar::new` checks the ABI.
    unsafe { Grammar::new(name, &path) }
        .with_context(|| format!("loading grammar '{name}' from {}", path.display()))
}
