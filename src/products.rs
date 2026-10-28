//! Product table, decoded from the `NSPopUpButton` menu in `Decoder.nib`.
//!
//! The suffix on each title selects the licensing scheme
//! (see `-[ASWRegDecoderRingWC generateCode:]`):
//!
//! | suffix | scheme                                   | internal mode |
//! |--------|------------------------------------------|---------------|
//! | (none) | old-style 8-letter code, variant 1       | 1             |
//! | `!`    | old-style 8-letter code, variant 2       | 2             |
//! | `*`    | RT3 time-stamped code                    | 3             |
//! | `**`   | RT3 code, 6-digit retail IDs accepted    | 3 + retail    |
//! | `?`    | externally licensed — no code generated  | 999           |
//!
//! Titles that start with a space are sub-items: the generator walks up the
//! menu until it finds a non-indented title, counting the steps (`depth`),
//! and uses that parent title (with the depth) to derive the algorithm.

pub const MENU: &[&str] = &[
    "Aki**",
    "Apeiron",
    "Apeiron X**",
    "Aquaria**",
    "Ares!",
    "Avara",
    "Barrack",
    "Bubble Trouble",
    "Bubble Trouble X**",
    "Chiral",
    "Coldstone*",
    "ColorSwitch",
    "ColorSwitch Pro",
    "Cythera!",
    "Darwinia*",
    "DEFCON?",
    "Deimos Rising*",
    "Dragster**",
    "EasyEnvelopes**",
    "Eclipse",
    "Eclipse 3",
    "El Ballo*",
    "Escape Velocity",
    "EV Nova**",
    "EV Override!",
    "Garendall*",
    "GooBall*",
    "Gooliens**",
    "Harry",
    "iSeek**",
    "iToner**",
    "Maelstrom",
    "Mondo Solitaire**",
    "Mars Rising!",
    "Oracle",
    "pop-pop**",
    "Redline**",
    "SketchFighter**",
    "Slithereens!",
    "Snapz",
    "Snapz Pro 2!",
    "Snapz Pro X**",
    " w/ Movie Capture 1.x",
    " w/ Movie Capture 2.x",
    "Soundboard**",
    "Soundboard 2**",
    "Swoop",
    "Uplink**",
    "WireTap Pro*",
    "WireTap Studio**",
    "WireTap Anywhere**",
    "WireTap Anywhere 2**",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    OldStyle1,
    OldStyle2,
    Rt3 { retail: bool },
    External,
}

impl Scheme {
    pub fn label(&self) -> &'static str {
        match self {
            Scheme::OldStyle1 => "old-style v1",
            Scheme::OldStyle2 => "old-style v2",
            Scheme::Rt3 { retail: false } => "RT3",
            Scheme::Rt3 { retail: true } => "RT3 (+retail IDs)",
            Scheme::External => "external (no code)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    /// What the user sees, e.g. "Snapz Pro X w/ Movie Capture 2.x".
    pub display: String,
    /// Name fed to the generator (parent title without suffix).
    pub key: String,
    /// Number of sub-item steps from the parent (algorithm variant).
    pub depth: i32,
    pub scheme: Scheme,
}

pub fn parse_suffix(title: &str) -> (String, Scheme) {
    if let Some(s) = title.strip_suffix('?') {
        (s.to_string(), Scheme::External)
    } else if let Some(s) = title.strip_suffix('!') {
        (s.to_string(), Scheme::OldStyle2)
    } else if let Some(s) = title.strip_suffix("**") {
        (s.to_string(), Scheme::Rt3 { retail: true })
    } else if let Some(s) = title.strip_suffix('*') {
        (s.to_string(), Scheme::Rt3 { retail: false })
    } else {
        (title.to_string(), Scheme::OldStyle1)
    }
}

/// Resolves menu item `index` the same way the original does.
pub fn resolve(index: usize) -> Product {
    let selected = MENU[index];
    let mut depth = 0usize;
    let mut title = selected;
    while title.starts_with(' ') {
        depth += 1;
        title = MENU[index - depth];
    }
    let (key, scheme) = parse_suffix(title);
    let display = if depth == 0 {
        key.clone()
    } else {
        format!("{}{}", key, selected)
    };
    Product { display, key, depth: depth as i32, scheme }
}

pub fn all() -> Vec<Product> {
    (0..MENU.len()).map(resolve).collect()
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Finds a product by display name (case/punctuation-insensitive).
pub fn find(query: &str) -> Result<Product, String> {
    let q = norm(query);
    let all = all();
    if let Some(p) = all.iter().find(|p| norm(&p.display) == q) {
        return Ok(p.clone());
    }
    let hits: Vec<_> = all.iter().filter(|p| norm(&p.display).contains(&q)).collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => Err(format!("unknown product '{query}' (see `decoder-ring list`)")),
        _ => Err(format!(
            "'{query}' is ambiguous: {}",
            hits.iter().map(|p| p.display.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subitems() {
        let p = find("Snapz Pro X w/ Movie Capture 2.x").unwrap();
        assert_eq!(p.key, "Snapz Pro X");
        assert_eq!(p.depth, 2);
        assert_eq!(p.scheme, Scheme::Rt3 { retail: true });
        let p = find("snapz pro x w/ movie capture 1").unwrap();
        assert_eq!(p.depth, 1);
        assert_eq!(find("Snapz Pro X").unwrap().depth, 0);
    }

    #[test]
    fn suffixes() {
        assert_eq!(find("DEFCON").unwrap().scheme, Scheme::External);
        assert_eq!(find("Ares").unwrap().scheme, Scheme::OldStyle2);
        assert_eq!(find("Coldstone").unwrap().scheme, Scheme::Rt3 { retail: false });
        assert_eq!(find("Escape Velocity").unwrap().scheme, Scheme::OldStyle1);
    }
}
