//! The one stylesheet, generated at runtime from `ade_core::gui::tokens`.
//!
//! The bake-off is only a fair comparison if both contenders paint the same
//! colours, so nothing here invents one: every custom property below is a
//! `ColorRole` resolved through `tokens::resolve`, and the table that maps
//! property name to role mirrors `ade-control-center`'s `theme.rs` `Palette`
//! construction field for field. A colour that differs between the two apps
//! would be a defect in this table, not a design choice.
//!
//! Appearance follows the OS: the light values sit on `:root` and the dark
//! values are re-declared inside `@media (prefers-color-scheme: dark)`, which
//! is the web platform's own version of egui's `set_visuals_of(Theme::…)`.
//!
//! Tones never become colours in view code either — a view asks for
//! [`tone_fill_class`] / [`tone_ink_class`] and gets a class name, the same
//! indirection `provider_list::tone_pair` provides on the egui side.

use ade_core::gui::tokens::{self, Appearance, ColorRole, Rgba};
use ade_core::gui::views::Tone;

/// The Lucide face, embedded rather than fetched — a desktop app has no
/// network at runtime and the bake-off must render identically offline.
static LUCIDE_TTF: &[u8] = include_bytes!("../../ade-control-center/assets/icons/lucide.ttf");

/// The icon font family name, referenced by every glyph span.
pub const FAMILY_ICONS: &str = "lucide";

/// Every custom property the stylesheet declares, paired with the semantic
/// role that supplies its value. The names are the short ones the views
/// read (`--text`, not `--default`); the roles are `theme.rs`'s mapping.
const TOKEN_VARS: [(&str, ColorRole); 34] = [
    ("bg", ColorRole::Bg),
    ("panel", ColorRole::Panel),
    ("inset", ColorRole::Inset),
    ("widget", ColorRole::Widget),
    ("widget-hover", ColorRole::WidgetHover),
    ("widget-active", ColorRole::WidgetActive),
    ("line", ColorRole::Line),
    ("hairline", ColorRole::Hairline),
    ("row-hover", ColorRole::RowHover),
    ("ghost-active", ColorRole::GhostActive),
    ("selected", ColorRole::Selected),
    ("selected-accent", ColorRole::SelectedAccent),
    ("text", ColorRole::Default),
    ("muted", ColorRole::Muted),
    ("faint", ColorRole::Faint),
    ("disabled", ColorRole::Disabled),
    ("accent", ColorRole::Accent),
    ("on-accent", ColorRole::OnAccent),
    ("ok", ColorRole::Success),
    ("warn", ColorRole::Warning),
    ("err", ColorRole::Error),
    ("info", ColorRole::Info),
    ("knob", ColorRole::Knob),
    ("toggle-track-off", ColorRole::ToggleTrackOff),
    ("scrim", ColorRole::Scrim),
    ("edge-highlight", ColorRole::EdgeHighlight),
    ("tint-ok", ColorRole::TintOk),
    ("tint-warn", ColorRole::TintWarn),
    ("tint-err", ColorRole::TintErr),
    ("tint-info", ColorRole::TintInfo),
    ("no-fill", ColorRole::NoFill),
    ("fill-hover-lift", ColorRole::FillHoverLift),
    ("fill-press-shade", ColorRole::FillPressShade),
    ("button-top-light", ColorRole::ButtonTopLight),
];

/// A token colour as a CSS `rgba()`. Alpha is emitted as a 0..1 fraction with
/// four decimals — enough that every distinct `u8` alpha maps to a distinct
/// string, so no two roles collapse into one value by rounding.
fn css_value(role: ColorRole, appearance: Appearance) -> String {
    let Rgba([r, g, b, a]) = tokens::resolve(role, appearance);
    format!("rgba({r}, {g}, {b}, {:.4})", a as f32 / 255.0)
}

/// The `--name: value;` declarations for one appearance, indented for the
/// block they land in.
fn declarations(appearance: Appearance) -> String {
    TOKEN_VARS
        .iter()
        .map(|(name, role)| format!("    --{name}: {};\n", css_value(*role, appearance)))
        .collect()
}

/// The background class for a tone — the CSS half of egui's `tone_pair`.
pub fn tone_fill_class(tone: Tone) -> &'static str {
    match tone {
        Tone::Ok => "fill-ok",
        Tone::Warn => "fill-warn",
        Tone::Err => "fill-err",
        Tone::Accent => "fill-accent",
        Tone::Neutral => "fill-neutral",
        Tone::Muted => "fill-muted",
    }
}

/// The foreground class for a tone. Status dots take it too and fill
/// themselves with `currentColor`, so a dot can never disagree with the
/// label beside it.
pub fn tone_ink_class(tone: Tone) -> &'static str {
    match tone {
        Tone::Ok => "ink-ok",
        Tone::Warn => "ink-warn",
        Tone::Err => "ink-err",
        Tone::Accent => "ink-accent",
        Tone::Neutral => "ink-neutral",
        Tone::Muted => "ink-muted",
    }
}

/// Standard base64, written out here because the four approved crates for
/// this contender do not include an encoder and one function is a smaller
/// cost than a dependency.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// The `@font-face` rule carrying the Lucide face as a data URL.
fn font_face() -> String {
    format!(
        "@font-face {{\n    font-family: {FAMILY_ICONS};\n    src: url(\"data:font/ttf;base64,{}\") format(\"truetype\");\n    font-display: block;\n}}\n",
        base64(LUCIDE_TTF)
    )
}

/// Layout, type and shape — everything that is not a colour. Sizes mirror the
/// egui twin's constants (`NAV_ROW_HEIGHT` 28, `SIDEBAR_WIDTH` 220,
/// `INSPECTOR_WIDTH` 360, `CONSOLE_HEIGHT` 200, `RADIUS_CONTROL` 5,
/// `RADIUS_CONTAINER` 10) so the two renders measure the same.
const LAYOUT: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; height: 100%; overflow: hidden; }
body {
    background: var(--bg);
    color: var(--text);
    font-family: -apple-system, "SF Pro Text", Inter, system-ui, sans-serif;
    font-size: 13px;
    -webkit-font-smoothing: antialiased;
}
button { font: inherit; cursor: default; }
button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.mono { font-family: "SF Mono", ui-monospace, Menlo, monospace; }
.glyph { font-family: lucide; line-height: 1; font-style: normal; }

/* tone -> colour, the CSS half of egui's tone_pair */
.fill-ok { background: var(--tint-ok); }
.fill-warn { background: var(--tint-warn); }
.fill-err { background: var(--tint-err); }
.fill-accent { background: var(--tint-info); }
.fill-neutral { background: var(--widget); }
.fill-muted { background: var(--widget); }
.ink-ok { color: var(--ok); }
.ink-warn { color: var(--warn); }
.ink-err { color: var(--err); }
.ink-accent { color: var(--accent); }
.ink-neutral { color: var(--muted); }
.ink-muted { color: var(--faint); }

/* shell: the sidebar runs the full window height; the console drawer spans
   only the content area to its right, as the egui twin's bottom panel does */
.app { display: flex; height: 100vh; }
.content { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.panes { flex: 1; min-height: 0; display: flex; }
.placeholder { flex: 1; padding: 24px; color: var(--muted); font-size: 13px; }

/* sidebar */
.sidebar {
    width: 220px; min-width: 220px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    display: flex; flex-direction: column;
    padding: 8px 8px 0;
}
.sidebar-scroll { flex: 1; min-height: 0; overflow-y: auto; }
.sidebar-footer { padding: 8px 8px 12px; color: var(--faint); font-size: 11px; }
.eyebrow {
    font-size: 11px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.8px; color: var(--muted);
    padding: 12px 8px 2px;
}
.eyebrow.first { padding-top: 4px; }
.nav-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; height: 28px; padding: 0 8px;
    border: 0; border-radius: 6px; background: transparent;
    color: var(--text); font-size: 13px; text-align: left;
}
.nav-row:hover { background: var(--row-hover); }
.nav-row.dimmed { color: var(--disabled); }
.nav-row.selected { background: var(--accent); color: #fff; font-weight: 600; }
.nav-icon { font-size: 16px; width: 16px; flex: none; color: var(--accent); }
.nav-row.dimmed .nav-icon { color: var(--disabled); }
.nav-row.selected .nav-icon { color: #fff; }
.nav-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pill {
    flex: none; min-width: 17px; height: 17px; border-radius: 9px;
    padding: 0 5px; font-size: 11px;
    display: inline-flex; align-items: center; justify-content: center;
}
.nav-row.selected .pill { background: rgba(255, 255, 255, 0.22); color: #fff; }

/* middle pane: the group page */
.middle { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 4px 16px 0; }
.page-title { font-size: 20px; font-weight: 600; margin: 4px 0 0; }
.page-subtitle { font-size: 11px; color: var(--muted); margin: 2px 0 12px; }
.segmented {
    align-self: flex-start; display: inline-flex;
    background: var(--widget); border-radius: 7px; padding: 2px; margin-bottom: 12px;
}
.segment {
    height: 24px; padding: 0 12px; font-size: 11px;
    border: 1px solid transparent; border-radius: 5px;
    background: transparent; color: var(--muted);
}
.segment:hover { background: var(--row-hover); }
.segment.active { background: var(--panel); border-color: var(--line); color: var(--text); }
.rows { flex: 1; min-height: 0; overflow-y: auto; }
.prow {
    display: flex; align-items: center; gap: 12px;
    width: 100%; height: 48px; padding: 0 12px;
    border: 0; border-radius: 5px; background: transparent;
    color: inherit; text-align: left;
}
.prow:hover { background: var(--row-hover); }
.prow.selected { background: var(--selected); }
.tile {
    flex: none; width: 28px; height: 28px; border-radius: 7px; font-size: 15px;
    display: flex; align-items: center; justify-content: center;
}
.prow-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.prow-line1 { display: flex; align-items: center; gap: 6px; min-width: 0; }
.prow-name { font-size: 13px; font-weight: 600; white-space: nowrap; }
.tag {
    flex: none; font-size: 9px; font-weight: 600; text-transform: uppercase;
    border-radius: 7px; padding: 1px 6px; letter-spacing: 0.3px;
}
.prow-desc {
    font-size: 11px; color: var(--muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.prow-version { flex: none; font-size: 11px; color: var(--muted); }
/* inset past the tile: 12 padding + 28 tile + 12 gap */
.hairline { height: 1px; background: var(--hairline); margin-left: 52px; }
.empty { padding: 24px 0; text-align: center; color: var(--muted); font-size: 13px; }

/* inspector */
.inspector {
    width: 360px; min-width: 360px;
    background: var(--bg); border-left: 1px solid var(--line);
    padding: 12px 20px; overflow-y: auto;
}
.insp-head { display: flex; align-items: center; gap: 8px; }
.insp-tile {
    flex: none; width: 40px; height: 40px; border-radius: 10px; font-size: 20px;
    display: flex; align-items: center; justify-content: center;
}
.insp-titles { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.insp-title { font-size: 20px; font-weight: 600; }
.insp-version { font-size: 13px; color: var(--muted); }
.insp-desc { font-size: 13px; color: var(--muted); margin-top: 6px; }
.actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 12px; }
.pill-btn {
    height: 26px; padding: 0 12px; font-size: 13px;
    border: 0; border-radius: 5px;
    background: var(--widget); color: var(--text);
}
.pill-btn:hover:not(:disabled) { background: var(--widget-hover); }
.pill-btn.destructive { background: var(--tint-err); color: var(--err); }
.pill-btn:disabled { color: var(--disabled); }
.guidance { margin-top: 8px; font-size: 11px; color: var(--muted); }
.last-action { margin-top: 6px; font-size: 11px; color: var(--faint); }
.card-heading {
    font-size: 10.5px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.8px; color: var(--faint); margin: 20px 0 4px;
}
.card {
    background: var(--panel); border-radius: 10px; padding: 12px 16px;
    display: grid; grid-template-columns: 1fr 1fr; gap: 12px 16px;
    margin-bottom: 16px;
}
.pair { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.pair-label { font-size: 11px; color: var(--muted); }
.pair-value { font-size: 13px; color: var(--text); overflow-wrap: anywhere; }
.pair-value.link { color: var(--info); text-decoration: none; }
.issue { display: flex; flex-wrap: wrap; gap: 6px; align-items: baseline; margin-bottom: 4px; }
.issue-level { font-size: 11px; font-weight: 600; }
.issue-message { font-size: 13px; }
.issue-fix { font-size: 11px; color: var(--muted); }

/* console drawer */
.console {
    height: 200px; min-height: 200px;
    background: var(--panel); border-top: 1px solid var(--line);
    padding: 8px 12px; display: flex; flex-direction: column;
}
.console-bar { display: flex; align-items: center; gap: 6px; }
.console-tabs { display: flex; gap: 6px; flex: 1; min-width: 0; overflow-x: auto; }
.ctab {
    flex: none; display: flex; align-items: center; gap: 6px;
    height: 22px; padding: 0 8px; font-size: 11px;
    border: 0; border-radius: 5px; background: transparent; color: var(--muted);
}
.ctab:hover { background: var(--row-hover); }
.ctab.active { color: var(--text); }
.dot { flex: none; width: 6px; height: 6px; border-radius: 3px; background: currentColor; }
.console-status { font-size: 11px; }
.ghost-btn {
    height: 24px; padding: 0 8px; font-size: 13px;
    border: 0; border-radius: 5px; background: transparent; color: var(--muted);
}
.ghost-btn:hover { background: var(--row-hover); }
.icon-btn {
    flex: none; width: 24px; height: 24px; font-size: 14px;
    border: 0; border-radius: 5px; background: transparent; color: var(--muted);
    display: flex; align-items: center; justify-content: center;
}
.icon-btn:hover { background: var(--row-hover); }
.console-body {
    flex: 1; min-height: 0; margin-top: 6px; overflow-y: auto;
    background: var(--inset); border-radius: 5px; padding: 8px 12px;
    font-size: 11px;
}
.console-line { white-space: pre-wrap; }
.console-empty { color: var(--faint); }
"#;

/// The complete stylesheet: embedded icon face, light tokens on `:root`,
/// dark tokens under the OS appearance query, then layout.
pub fn stylesheet() -> String {
    format!(
        "{}\n:root {{\n{}}}\n\n@media (prefers-color-scheme: dark) {{\n  :root {{\n{}  }}\n}}\n{LAYOUT}",
        font_face(),
        declarations(Appearance::Light),
        declarations(Appearance::Dark),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ISC for this contender's token layer: every declared custom property
    /// resolves to a real colour in BOTH appearances. A role that only
    /// resolved in one would leave the other appearance painting with an
    /// inherited or missing value — the exact class of bug a generated
    /// stylesheet exists to make impossible.
    #[test]
    fn every_token_variable_has_a_value_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            for (name, role) in TOKEN_VARS {
                let value = css_value(role, appearance);
                assert!(
                    value.starts_with("rgba(") && value.ends_with(')'),
                    "--{name} in {appearance:?} is not an rgba() value: {value}"
                );
                // Four numeric fields, none of them empty.
                let inner = value
                    .trim_start_matches("rgba(")
                    .trim_end_matches(')')
                    .to_string();
                let fields: Vec<&str> = inner.split(", ").collect();
                assert_eq!(fields.len(), 4, "--{name} in {appearance:?}: {value}");
                assert!(
                    fields.iter().all(|f| !f.is_empty()),
                    "--{name} in {appearance:?} has an empty channel: {value}"
                );
            }
        }
    }

    /// Both blocks are actually emitted, and every variable appears in each —
    /// the table above is only a promise until the sheet carries it twice.
    #[test]
    fn the_stylesheet_declares_every_variable_in_both_blocks() {
        let css = stylesheet();
        let (light_block, rest) = css
            .split_once("@media (prefers-color-scheme: dark)")
            .expect("the sheet carries a dark-appearance media query");
        for (name, _) in TOKEN_VARS {
            let decl = format!("--{name}:");
            assert!(
                light_block.contains(&decl),
                "--{name} missing from the light :root block"
            );
            assert!(
                rest.contains(&decl),
                "--{name} missing from the dark media block"
            );
        }
    }

    /// Light and dark are genuinely different sheets — if the two blocks were
    /// byte-identical the media query would be decoration.
    #[test]
    fn the_two_appearance_blocks_differ() {
        assert_ne!(
            declarations(Appearance::Light),
            declarations(Appearance::Dark)
        );
    }

    /// Tones map to classes the sheet actually defines, for all six variants.
    #[test]
    fn every_tone_maps_to_a_class_the_sheet_defines() {
        let css = stylesheet();
        for tone in [
            Tone::Neutral,
            Tone::Muted,
            Tone::Accent,
            Tone::Ok,
            Tone::Warn,
            Tone::Err,
        ] {
            for class in [tone_fill_class(tone), tone_ink_class(tone)] {
                assert!(
                    css.contains(&format!(".{class} {{")),
                    "{tone:?} maps to .{class}, which the sheet does not define"
                );
            }
        }
    }

    /// The icon face ships inside the sheet — no runtime file read, no
    /// network, so the render is identical on any machine.
    #[test]
    fn the_icon_font_is_embedded_as_a_data_url() {
        let css = stylesheet();
        assert!(css.contains("@font-face"));
        assert!(css.contains("data:font/ttf;base64,"));
        assert!(!LUCIDE_TTF.is_empty());
    }

    /// The hand-rolled encoder, against RFC 4648's own vectors — including
    /// every padding case, which is where a hand-rolled base64 goes wrong.
    #[test]
    fn base64_matches_the_rfc_4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // The 62/63 alphabet entries only appear for high bytes; a truncated
        // ALPHABET would still pass the ASCII vectors above.
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
    }
}
