# dathan

Highlight source code to HTML or Clojure/EDN Hiccup using Helix’s tree-sitter
grammars and queries.

It reads the precompiled grammars and `highlights.scm` / `injections.scm` /
`locals.scm` queries from a Helix runtime, so it covers whatever languages that
runtime provides. Grammar loading, query `inherits` resolution, injections, and
locals are handled by [tree-house](https://github.com/helix-editor/tree-house),
the same library Helix uses.

## Build

```
cargo build --release
```

The binary is `target/release/dathan`.

## Usage

```
dathan [OPTIONS] [FILE]
```

If `FILE` is omitted, source is read from stdin. With stdin there is no filename
to detect from, so pass `--lang` (or rely on a `#!` shebang line).

Options:

```
--format <html|edn-hiccup>   Output format. Default: edn-hiccup.
--lang <name>                Force a language instead of detecting it.
--runtime <path>             Extra runtime root, highest priority. Repeatable.
--languages <path>           Base languages.toml. The user config is still merged on top.
--theme <path>               theme.toml for --emit-css.
--emit-css                   Write a CSS stylesheet from the theme and exit. Ignores FILE.
-o, --output <path>          Output file. Default: stdout.
```

Examples:

```
dathan src/main.rs
dathan --format html src/main.rs -o main.html
cat src/main.rs | dathan --lang rust
dathan --emit-css --theme ~/source/helix/theme.toml -o theme.css
```

## Runtime discovery

Runtime roots are searched in this order, first match wins:

1. `--runtime` paths
2. `$HELIX_RUNTIME`
3. `~/.config/helix/runtime`

The language registry is read from `~/source/helix/languages.toml` (or
`--languages`), with `~/.config/helix/languages.toml` merged over it by language
name.

Language detection uses the file extension, then `file-types` globs, then a
`#!` shebang line. Override with `--lang`.

## Output

Both formats name spans by scope. A dotted scope becomes space-separated
hierarchical classes, so `keyword.control.conditional` is rendered as
`keyword keyword-control keyword-control-conditional`.

HTML:

```html
<div class="dathan">
  <pre><code><span class="keyword keyword-function">fn</span> …</code></pre>
</div>
```

EDN Hiccup:

```clojure
[:div.dathan [:pre [:code [:span {:class "keyword keyword-function"} "fn"] " " …]]]
```

CSS from `--emit-css` targets the most specific class, for example
`.keyword-control { color: …; }`. Palette names in the theme are resolved.

## Tests

```
cargo test
```

## License

Copyright © 2026 Tom Waddington

Distributed under the MIT License. See LICENSE file for details.
