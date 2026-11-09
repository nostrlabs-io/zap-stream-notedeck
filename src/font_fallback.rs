use egui::{FallbackRequest, FontInsert, FontProvider};
use std::sync::OnceLock;

fn han_stand_in(c: char) -> Option<&'static str> {
    matches!(c as u32, 0x3000..=0x303F | 0x30FB..=0x30FC | 0xFF00..=0xFFEF).then_some("日")
}

fn cjk_font_for(system: &dyn FontProvider, request: &FallbackRequest<'_>) -> Option<FontInsert> {
    let cluster = han_stand_in(request.cluster.chars().next()?)?;
    system.font_for(&FallbackRequest {
        cluster,
        family: request.family,
    })
}

#[derive(Default)]
pub struct CjkPunctuation {
    system: OnceLock<egui_system_fonts::SystemFontProvider>,
}

impl FontProvider for CjkPunctuation {
    fn font_for(&self, request: &FallbackRequest<'_>) -> Option<FontInsert> {
        han_stand_in(request.cluster.chars().next()?)?;
        let system = self.system.get_or_init(egui_system_fonts::SystemFontProvider::new);
        cjk_font_for(system, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn fullwidth_punctuation_asks_for_a_han_font() {
        let asked = Mutex::new(Vec::new());
        let system = |r: &FallbackRequest<'_>| {
            asked.lock().unwrap().push(r.cluster.to_string());
            None
        };
        for c in ['，', '：', '（', '、', '。', 'ー', '\u{3000}'] {
            let s = c.to_string();
            cjk_font_for(&system, &FallbackRequest { cluster: &s, family: &egui::FontFamily::Proportional });
        }
        assert_eq!(*asked.lock().unwrap(), vec!["日"; 7]);
    }

    #[test]
    fn other_characters_are_left_to_the_system_provider() {
        let system = |_: &FallbackRequest<'_>| -> Option<FontInsert> { panic!("asked") };
        for c in ['a', '«', '…', '“', 'Ж', 'λ', '日', 'ア', '한'] {
            let s = c.to_string();
            assert!(cjk_font_for(&system, &FallbackRequest { cluster: &s, family: &egui::FontFamily::Proportional }).is_none());
        }
    }
}
