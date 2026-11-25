use egui::epaint::{FontFamily, FontId};
use egui::{Color32, Margin, Stroke, TextStyle};
use std::sync::Arc;

pub const FONT_SIZE: f32 = 14.5;
pub const FONT_SIZE_SM: f32 = 13.0;
pub const FONT_SIZE_LG: f32 = 19.0;
pub const ROUNDING_DEFAULT: f32 = 12.0;
pub const ROUNDING_LG: f32 = 16.0;
pub const MARGIN_DEFAULT: Margin = Margin::symmetric(12, 6);

pub const BG: Color32 = Color32::from_rgb(13, 12, 15);
pub const SURFACE: Color32 = Color32::from_rgb(32, 29, 38);
pub const SURFACE_LOW: Color32 = Color32::from_rgb(22, 20, 27);
pub const BORDER: Color32 = Color32::from_rgb(44, 41, 51);
pub const TEXT: Color32 = Color32::from_rgb(242, 238, 232);
pub const TEXT_SOFT: Color32 = Color32::from_rgb(201, 196, 204);
pub const MUTED: Color32 = Color32::from_rgb(154, 149, 163);
pub const PRIMARY: Color32 = Color32::from_rgb(255, 176, 0);
pub const ON_PRIMARY: Color32 = Color32::from_rgb(26, 17, 0);
pub const ZAP: Color32 = PRIMARY;

pub const NEUTRAL_500: Color32 = MUTED;
pub const NEUTRAL_700: Color32 = BORDER;
pub const NEUTRAL_800: Color32 = SURFACE;
pub const NEUTRAL_900: Color32 = SURFACE_LOW;

pub const SIDEBAR_WIDTH: f32 = 208.0;
pub const HEADER_HEIGHT: f32 = 56.0;
pub const WIDE_BREAKPOINT: f32 = 800.0;

pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn display_bold() -> FontFamily {
    FontFamily::Name("display-bold".into())
}

pub fn medium_family() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    apply_style(ctx);
}

const MONOCHROME_EMOJI_FONTS: [&str; 2] = ["NotoEmoji-Regular", "emoji-icon-font"];

pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

const COLOUR_EMOJI_PATHS: [&str; 6] = [
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/Noto-COLRv1.ttf",
    "/system/fonts/NotoColorEmoji.ttf",
    "/System/Library/Fonts/Apple Color Emoji.ttc",
    "C:\\Windows\\Fonts\\seguiemj.ttf",
];

const COLOUR_EMOJI: &str = "colour-emoji";

fn load_colour_emoji() -> Option<Vec<u8>> {
    COLOUR_EMOJI_PATHS
        .iter()
        .find_map(|p| std::fs::read(p).ok())
}

const SYMBOL_FONT_PATHS: [(&str, &[&str]); 2] = [
    (
        "symbols-1",
        &[
            "/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
        ],
    ),
    (
        "symbols-2",
        &[
            "/system/fonts/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        ],
    ),
];

fn load_symbol_fonts() -> Vec<(String, Vec<u8>)> {
    SYMBOL_FONT_PATHS
        .iter()
        .filter_map(|(name, paths)| {
            paths
                .iter()
                .find_map(|p| std::fs::read(p).ok())
                .map(|bytes| (name.to_string(), bytes))
        })
        .collect()
}

pub fn font_definitions() -> egui::FontDefinitions {
    font_definitions_with(load_colour_emoji(), load_symbol_fonts())
}

fn font_definitions_with(
    colour_emoji: Option<Vec<u8>>,
    symbol_fonts: Vec<(String, Vec<u8>)>,
) -> egui::FontDefinitions {
    let mut fd = egui::FontDefinitions::default();
    let load = |fd: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]| {
        fd.font_data
            .insert(name.to_string(), Arc::new(egui::FontData::from_static(bytes)));
    };
    load(
        &mut fd,
        "InstrumentSans-Regular",
        include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf"),
    );
    load(
        &mut fd,
        "InstrumentSans-Medium",
        include_bytes!("../assets/fonts/InstrumentSans-Medium.ttf"),
    );
    load(
        &mut fd,
        "InstrumentSans-SemiBold",
        include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf"),
    );
    load(
        &mut fd,
        "Bricolage-Bold",
        include_bytes!("../assets/fonts/Bricolage-Bold.ttf"),
    );
    load(
        &mut fd,
        "Bricolage-ExtraBold",
        include_bytes!("../assets/fonts/Bricolage-ExtraBold.ttf"),
    );

    let fallback = fd
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !MONOCHROME_EMOJI_FONTS.contains(&name.as_str()))
        .collect::<Vec<_>>();
    let has_colour = colour_emoji.is_some();
    if let Some(bytes) = colour_emoji {
        fd.font_data
            .insert(COLOUR_EMOJI.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
    }
    let symbol_names = symbol_fonts
        .into_iter()
        .map(|(name, bytes)| {
            fd.font_data
                .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            name
        })
        .collect::<Vec<_>>();
    let with_fallback = |first: &str| {
        let mut v = vec![first.to_string()];
        if has_colour {
            v.push(COLOUR_EMOJI.to_string());
        }
        v.extend(symbol_names.iter().cloned());
        v.extend(fallback.iter().cloned());
        v
    };
    fd.families
        .insert(FontFamily::Proportional, with_fallback("InstrumentSans-Regular"));
    fd.families
        .insert(medium_family(), with_fallback("InstrumentSans-Medium"));
    fd.families
        .insert(semibold_family(), with_fallback("InstrumentSans-SemiBold"));
    fd.families
        .insert(display_bold(), with_fallback("Bricolage-Bold"));
    fd.families
        .insert(display_family(), with_fallback("Bricolage-ExtraBold"));
    fd
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(FONT_SIZE, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(FONT_SIZE, medium_family()),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(30.72, display_family()),
        );
        let v = &mut style.visuals;
        v.panel_fill = BG;
        v.window_fill = SURFACE_LOW;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.extreme_bg_color = SURFACE_LOW;
        v.faint_bg_color = SURFACE_LOW;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = PRIMARY;
        v.selection.bg_fill = PRIMARY.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, PRIMARY);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.inactive.bg_fill = SURFACE;
        v.widgets.inactive.weak_bg_fill = SURFACE;
        v.widgets.hovered.bg_fill = BORDER;
        v.widgets.hovered.weak_bg_fill = BORDER;
    });
}

pub fn display(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, display_family())).color(TEXT)
}

pub fn display_bold_text(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, display_bold())).color(TEXT)
}

pub fn medium(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, medium_family()))
}

pub fn semibold(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, semibold_family()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_emoji_font_precedes_system_fallbacks() {
        let fd = font_definitions_with(Some(vec![0]), Vec::new());
        for fonts in fd.families.values().filter(|f| f.iter().any(|n| n == "Hack") == false) {
            assert_eq!(fonts[1], COLOUR_EMOJI);
        }
    }

    #[test]
    fn symbol_fonts_follow_colour_emoji_in_every_family() {
        let fd = font_definitions_with(
            Some(vec![0]),
            vec![("symbols-1".into(), vec![0]), ("symbols-2".into(), vec![0])],
        );
        for fonts in fd.families.values().filter(|f| f.iter().any(|n| n == "InstrumentSans-Regular" || n.starts_with("Bricolage") || n.starts_with("InstrumentSans"))) {
            assert_eq!(fonts[1], COLOUR_EMOJI);
            assert_eq!(fonts[2], "symbols-1");
            assert_eq!(fonts[3], "symbols-2");
        }
    }

    #[test]
    fn dingbat_resolves_to_a_system_font() {
        let loaded = load_symbol_fonts();
        if loaded.is_empty() {
            return;
        }
        let covers = loaded.iter().any(|(_, bytes)| {
            ttf_has_glyph(bytes, '\u{274B}')
        });
        assert!(covers, "no symbol font covers U+274B");
    }

    fn ttf_has_glyph(bytes: &[u8], c: char) -> bool {
        let data = egui::FontData::from_owned(bytes.to_vec());
        let mut fd = egui::FontDefinitions::empty();
        fd.font_data.insert("f".into(), Arc::new(data));
        fd.families.insert(FontFamily::Proportional, vec!["f".into()]);
        let ctx = egui::Context::default();
        ctx.set_fonts(fd);
        let _ = ctx.run_ui(Default::default(), |_| {});
        ctx.fonts_mut(|f| f.has_glyph(&FontId::proportional(14.0), c))
    }

    #[test]
    fn emoji_fall_through_to_system_colour_font() {
        let fd = font_definitions();
        for (family, fonts) in &fd.families {
            for mono in MONOCHROME_EMOJI_FONTS {
                assert!(
                    !fonts.iter().any(|f| f == mono),
                    "{family:?} still lists monochrome {mono}"
                );
            }
        }
    }
}
