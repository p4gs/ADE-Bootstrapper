//! Lucide glyph resolution.
//!
//! Core decides WHICH icon a thing gets (`ade_core::gui::views::Icon`, by
//! Lucide name); this is the only place in the contender where a name becomes
//! a codepoint, read from the same `codepoints.json` the egui twin ships. The
//! font itself is embedded by `styles::stylesheet` as an `@font-face` data
//! URL, so a glyph and its face can never come from different builds.

use ade_core::gui::views::Icon;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

static CODEPOINTS_JSON: &str =
    include_str!("../../ade-control-center/assets/icons/codepoints.json");

fn codepoint_table() -> &'static HashMap<String, u32> {
    static TABLE: OnceLock<HashMap<String, u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let parsed: Value =
            serde_json::from_str(CODEPOINTS_JSON).expect("assets/icons/codepoints.json parses");
        parsed
            .as_object()
            .expect("codepoints.json is a JSON object")
            .iter()
            .filter_map(|(name, value)| {
                // The map is name -> decimal codepoint. Anything that is not a
                // u32 is skipped rather than panicking: a malformed entry for
                // an icon nobody uses must not take the whole app down, and
                // the test below proves every icon we DO use is present.
                let cp = value.as_u64()?;
                Some((name.clone(), u32::try_from(cp).ok()?))
            })
            .collect()
    })
}

/// The glyph for an icon. A name missing from the shipped map is a build
/// defect (the test below enumerates every variant); at runtime it renders as
/// the replacement character rather than a wrong icon.
pub fn glyph(icon: Icon) -> char {
    codepoint_table()
        .get(icon.lucide_name())
        .and_then(|&cp| char::from_u32(cp))
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon core can ask for resolves to a real private-use glyph in the
    /// shipped map. Outside U+E000..U+F8FF means the name matched something
    /// that is not an icon-font codepoint at all.
    #[test]
    fn every_icon_name_resolves_to_a_private_use_glyph() {
        for icon in Icon::ALL {
            let g = glyph(icon);
            assert_ne!(
                g,
                char::REPLACEMENT_CHARACTER,
                "{} is missing from codepoints.json",
                icon.lucide_name()
            );
            let cp = g as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&cp),
                "{} resolved outside the private-use area: {cp:#x}",
                icon.lucide_name()
            );
        }
    }

    /// Two icons sharing a glyph would render as the same mark — a silent
    /// visual collision the type system cannot catch.
    #[test]
    fn distinct_icons_never_share_a_glyph() {
        let mut seen = std::collections::HashSet::new();
        for icon in Icon::ALL {
            assert!(
                seen.insert(glyph(icon)),
                "{} duplicates another icon's glyph",
                icon.lucide_name()
            );
        }
    }
}
