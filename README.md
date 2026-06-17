# dathan

Highlight source code to HTML, Hiccup or ANSI, using
[Helix](https://helix-editor.com/)’s tree-sitter grammars and queries.

`dathan` reads the compiled grammars and `highlights.scm` / `injections.scm` /
`locals.scm` queries from a Helix runtime, so it covers whatever languages that
runtime provides. Grammar loading, query `inherits` resolution, injections, and
locals are handled by [tree-house](https://github.com/helix-editor/tree-house),
the same library Helix uses.

## Build

```sh
cargo build --release
```

The binary is `target/release/dathan`.

Or install directly with:

```sh
cargo install --path .
```

## Usage

```sh
dathan [OPTIONS] [FILE]
```

If `FILE` is omitted, source is read from `stdin`. With `stdin` there is no filename
to detect from, so pass `--lang` (or rely on a `#!` shebang line).

Options:

```
--format <FORMAT>            Output format. Default: terminal. See below.
--inline                     For class-based formats, emit theme-resolved inline styles.
--lang <name>                Force a language instead of detecting it.
--runtime <path>             Extra runtime root, highest priority. Repeatable.
--languages <path>           Base languages.toml. The user config is still merged on top.
--theme <path>               theme.toml for --emit-css and the theme-aware output.
--emit-css                   Write a CSS stylesheet from the theme and exit. Ignores FILE.
-o, --output <path>          Output file. Default: stdout.
```

Formats:

| `--format`    | Output                                         |
| ------------- | ---------------------------------------------- |
| `terminal`    | ANSI escape codes, 24-bit colour (default).    |
| `html`        | `<pre><code>` with hierarchical `class`.       |
| `edn-hiccup`  | Clojure/EDN Hiccup with hierarchical `:class`. |
| `json-hiccup` | JSON Hiccup arrays with hierarchical `class`.  |

The three class-based formats (`html`, `edn-hiccup`, `json-hiccup`) name spans
with hierarchical `class`es for styling via an external stylesheet (see
`--emit-css`).

Passing `--inline` instead resolves each scope to an inline `style` from the
theme and puts a base `ui.text` / `ui.background` style on the container.

`--inline` and the `terminal` format resolve colours and modifiers from the
theme directly. They use `--theme` if given, otherwise the same theme discovery
as `--emit-css`, and fall back to the bundled `default` theme when none is found.

A scope’s style is resolved by longest dotted prefix (e.g. `function.builtin`
falls back to `function`), matching Helix.

Examples:

```sh
dathan src/main.rs
dathan --format html src/main.rs -o main.html
cat src/main.rs | dathan --lang rust
dathan --emit-css --theme ~/source/helix/theme.toml -o theme.css
dathan --format terminal --theme ~/source/helix/theme.toml src/main.rs
dathan --format html --inline src/main.rs -o main.html
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

The class-based formats name spans by scope. A dotted scope becomes
space-separated hierarchical classes, so `keyword.control.conditional` is
rendered as `keyword keyword-control keyword-control-conditional`.

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

JSON Hiccup:

```json
["div",{"class":"dathan"},["pre",{},["code",{},["span",{"class":"keyword keyword-function"},"fn"]," ",…]]]
```

CSS from `--emit-css` targets the most specific class, for example
`.keyword-control { color: …; }`. Palette names in the theme are resolved.

With `--inline`, the same class-based formats bake the resolved style into each
span instead. Inline HTML:

```html
<div class="dathan">
  <pre style="color: #a4a0e8; background-color: #3b224c"><code>
    <span style="color: #eccdba">fn</span> …
  </code></pre>
</div>
```

`terminal` emits the same spans as ANSI SGR escape codes (24-bit colour), with a
base colour from the theme’s `ui.text` / `ui.background`.

## Tests

```sh
cargo test
```

## License

Copyright © 2026 Tom Waddington

Distributed under the MIT License. See LICENSE file for details.
