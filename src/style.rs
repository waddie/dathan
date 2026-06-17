//! Palette-resolved visual styles, shared by the CSS emitter and the
//! theme-aware backends (terminal, inline HTML).
//!
//! Colours are kept as concrete strings (`#rrggbb` or a named colour) after
//! palette resolution: CSS and inline HTML emit them verbatim, while the
//! terminal backend parses them into 24-bit SGR colour parameters.

/// A text modifier from a Helix theme.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Bold,
    Italic,
    Underlined,
    CrossedOut,
    Dim,
}

impl Modifier {
    /// Parse a Helix `modifiers` entry, ignoring unknown names.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "bold" => Some(Self::Bold),
            "italic" => Some(Self::Italic),
            "underlined" => Some(Self::Underlined),
            "crossed_out" => Some(Self::CrossedOut),
            "dim" => Some(Self::Dim),
            _ => None,
        }
    }
}

/// A resolved style: palette-resolved colours plus normalised modifiers.
#[derive(Clone, Default, PartialEq)]
pub struct Style {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub modifiers: Vec<Modifier>,
}

impl Style {
    pub fn is_empty(&self) -> bool {
        self.fg.is_none() && self.bg.is_none() && self.modifiers.is_empty()
    }

    /// Add a modifier unless already present (preserving order).
    pub fn push_modifier(&mut self, m: Modifier) {
        if !self.modifiers.contains(&m) {
            self.modifiers.push(m);
        }
    }

    /// CSS declarations like `color: #rrggbb`, in the order the CSS emitter and
    /// the inline-style backend expect (colour, background, then modifiers).
    pub fn css_declarations(&self) -> Vec<String> {
        let mut decls = Vec::new();
        if let Some(fg) = &self.fg {
            decls.push(format!("color: {fg}"));
        }
        if let Some(bg) = &self.bg {
            decls.push(format!("background-color: {bg}"));
        }
        for m in &self.modifiers {
            decls.push(
                match m {
                    Modifier::Bold => "font-weight: bold",
                    Modifier::Italic => "font-style: italic",
                    Modifier::Underlined => "text-decoration: underline",
                    Modifier::CrossedOut => "text-decoration: line-through",
                    Modifier::Dim => "opacity: 0.7",
                }
                .to_string(),
            );
        }
        decls
    }

    /// SGR parameter numbers (without the `\x1b[` prefix / `m` suffix) for a
    /// terminal, using 24-bit truecolor for colours.
    pub fn sgr_params(&self) -> Vec<String> {
        let mut params = Vec::new();
        if let Some((r, g, b)) = self.fg.as_deref().and_then(parse_color) {
            params.push(format!("38;2;{r};{g};{b}"));
        }
        if let Some((r, g, b)) = self.bg.as_deref().and_then(parse_color) {
            params.push(format!("48;2;{r};{g};{b}"));
        }
        for m in &self.modifiers {
            params.push(
                match m {
                    Modifier::Bold => "1",
                    Modifier::Dim => "2",
                    Modifier::Italic => "3",
                    Modifier::Underlined => "4",
                    Modifier::CrossedOut => "9",
                }
                .to_string(),
            );
        }
        params
    }
}

/// Parse a colour string into RGB: `#rgb` / `#rrggbb`, or one of the standard
/// named colours. Unknown values yield `None` (no colour emitted).
fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    named_rgb(s)
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    match hex.len() {
        3 => {
            let mut it = hex.chars().map(|c| c.to_digit(16));
            let r = it.next()?? as u8;
            let g = it.next()?? as u8;
            let b = it.next()?? as u8;
            // Expand `#abc` to `#aabbcc`.
            Some((r * 17, g * 17, b * 17))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// The standard 16 ANSI colours, using common xterm RGB values.
fn named_rgb(name: &str) -> Option<(u8, u8, u8)> {
    let rgb = match name {
        "black" => (0, 0, 0),
        "red" => (205, 0, 0),
        "green" => (0, 205, 0),
        "yellow" => (205, 205, 0),
        "blue" => (0, 0, 238),
        "magenta" => (205, 0, 205),
        "cyan" => (0, 205, 205),
        "white" | "gray" | "grey" => (229, 229, 229),
        "light-gray" | "light-grey" => (127, 127, 127),
        "light-red" => (255, 0, 0),
        "light-green" => (0, 255, 0),
        "light-yellow" => (255, 255, 0),
        "light-blue" => (92, 92, 255),
        "light-magenta" => (255, 0, 255),
        "light-cyan" => (0, 255, 255),
        "light-white" => (255, 255, 255),
        _ => return None,
    };
    Some(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_declaration_order() {
        let style = Style {
            fg: Some("#0000ff".into()),
            bg: Some("#111111".into()),
            modifiers: vec![Modifier::Bold, Modifier::Italic],
        };
        assert_eq!(
            style.css_declarations(),
            vec![
                "color: #0000ff",
                "background-color: #111111",
                "font-weight: bold",
                "font-style: italic",
            ]
        );
    }

    #[test]
    fn sgr_truecolor_and_modifiers() {
        let style = Style {
            fg: Some("#ff8040".into()),
            bg: None,
            modifiers: vec![Modifier::Bold, Modifier::Underlined],
        };
        assert_eq!(style.sgr_params(), vec!["38;2;255;128;64", "1", "4"]);
    }

    #[test]
    fn hex_and_named_colors() {
        assert_eq!(parse_color("#abc"), Some((170, 187, 204)));
        assert_eq!(parse_color("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_color("blue"), Some((0, 0, 238)));
        assert_eq!(parse_color("not-a-color"), None);
    }
}
