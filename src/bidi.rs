use std::borrow::Cow;
use unicode_bidi::{BidiInfo, Level};
use unicode_bidi_mirroring::get_mirrored;

struct Token {
    start: usize,
    end: usize,
    level: Level,
}

fn tokens(text: &str, levels: &[Level], start: usize, end: usize) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut last_space = false;
    for (i, c) in text[start..end].char_indices() {
        let at = start + i;
        let level = levels[at];
        let space = c.is_whitespace();
        match out.last_mut() {
            Some(t) if t.level == level && space == last_space => t.end = at + c.len_utf8(),
            _ => out.push(Token {
                start: at,
                end: at + c.len_utf8(),
                level,
            }),
        }
        last_space = space;
    }
    out
}

fn push_token(out: &mut String, text: &str, token: &Token) {
    let slice = &text[token.start..token.end];
    if token.level.is_rtl() {
        out.extend(slice.chars().map(|c| get_mirrored(c).unwrap_or(c)));
    } else {
        out.push_str(slice);
    }
}

pub fn visual(text: &str) -> Cow<'_, str> {
    if text.is_empty() || text.is_ascii() {
        return Cow::Borrowed(text);
    }
    let info = BidiInfo::new(text, None);
    if !info.has_rtl() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for para in &info.paragraphs {
        let line_end = if para.range.end > para.range.start
            && text[para.range.clone()].ends_with('\n')
        {
            para.range.end - 1
        } else {
            para.range.end
        };
        let toks = tokens(text, &info.levels, para.range.start, line_end);
        let levels: Vec<Level> = toks.iter().map(|t| t.level).collect();
        for idx in BidiInfo::reorder_visual(&levels) {
            push_token(&mut out, text, &toks[idx]);
        }
        out.push_str(&text[line_end..para.range.end]);
    }
    Cow::Owned(out)
}

pub fn start_layout(valign: egui::Align) -> egui::Layout {
    if crate::i18n::is_rtl() {
        egui::Layout::right_to_left(valign)
    } else {
        egui::Layout::left_to_right(valign)
    }
}

pub fn end_layout(valign: egui::Align) -> egui::Layout {
    if crate::i18n::is_rtl() {
        egui::Layout::left_to_right(valign)
    } else {
        egui::Layout::right_to_left(valign)
    }
}

pub fn text_align() -> egui::Align {
    if crate::i18n::is_rtl() {
        egui::Align::Max
    } else {
        egui::Align::Min
    }
}

pub fn flip_icon(image: egui::Image<'static>) -> egui::Image<'static> {
    if crate::i18n::is_rtl() {
        image.uv(egui::Rect::from_min_max(egui::pos2(1.0, 0.0), egui::pos2(0.0, 1.0)))
    } else {
        image
    }
}

pub fn vertical<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    ui.scope_builder(
        egui::UiBuilder::new().layout(egui::Layout::top_down(text_align())),
        add,
    )
}

pub fn visual_rows(rows: &[String]) -> String {
    rows.iter()
        .map(|r| visual(r.trim_end()).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn wrap_logical(text: &str, width: f32, measure: &mut dyn FnMut(&str) -> f32) -> Vec<String> {
    let mut rows = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(&visual(&candidate)) > width {
                rows.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        rows.push(line);
    }
    rows
}

pub fn wrap_rows(ctx: &egui::Context, text: &str, font: egui::FontId, width: f32) -> String {
    let mut measure = |s: &str| {
        ctx.fonts_mut(|f| {
            f.layout_no_wrap(s.to_string(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        })
    };
    visual_rows(&wrap_logical(text, width, &mut measure))
}

pub fn wrapped_label(ui: &mut egui::Ui, text: &str, font: egui::FontId, color: egui::Color32) -> egui::Response {
    if crate::i18n::is_rtl() && is_rtl_text(text) {
        let shown = wrap_rows(ui.ctx(), text, font.clone(), ui.available_width());
        ui.add(
            egui::Label::new(egui::RichText::new(shown).font(font).color(color))
                .wrap_mode(egui::TextWrapMode::Extend)
                .halign(egui::Align::Max),
        )
    } else {
        ui.add(
            egui::Label::new(egui::RichText::new(text).font(font).color(color))
                .wrap_mode(egui::TextWrapMode::Wrap),
        )
    }
}

pub fn is_rtl_text(text: &str) -> bool {
    !text.is_ascii() && BidiInfo::new(text, None).has_rtl()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_latin_is_untouched() {
        assert!(matches!(visual("hello world"), Cow::Borrowed(_)));
        assert!(matches!(visual("Привет мир"), Cow::Borrowed(_)));
    }

    #[test]
    fn arabic_words_are_reordered_right_to_left() {
        let logical = "شاهد البث المباشر";
        let shown = visual(logical);
        let words: Vec<&str> = shown.split(' ').collect();
        assert_eq!(words, ["المباشر", "البث", "شاهد"]);
    }

    #[test]
    fn embedded_latin_keeps_its_own_order() {
        let shown = visual("مرحبا hello world يا");
        let words: Vec<&str> = shown.split(' ').collect();
        assert_eq!(words, ["يا", "hello", "world", "مرحبا"]);
    }

    #[test]
    fn numbers_stay_readable_inside_arabic() {
        let shown = visual("يشاهد 12 مستخدم");
        let words: Vec<&str> = shown.split(' ').collect();
        assert_eq!(words, ["مستخدم", "12", "يشاهد"]);
    }

    #[test]
    fn brackets_are_mirrored_in_rtl_runs() {
        let shown = visual("مرحبا ) يا");
        assert_eq!(shown, "يا ( مرحبا");
    }

    #[test]
    fn lines_are_reordered_independently() {
        let shown = visual("أ ب\nج د");
        assert_eq!(shown, "ب أ\nد ج");
    }

    const LONG: &str = "مادي بعملة ساتوشي لا حاجة إلى بريد إلكتروني أو كلمة مرور لإنشاء حسابك الجديد";

    #[test]
    fn wrapped_rows_keep_logical_order_per_line() {
        let words: Vec<&str> = LONG.split(' ').collect();
        let rows: Vec<String> = words.chunks(4).map(|c| c.join(" ")).collect();
        let shown = visual_rows(&rows);
        let lines: Vec<&str> = shown.split('\n').collect();
        assert_eq!(lines.len(), rows.len());
        let restored: Vec<String> = lines.iter().map(|l| visual(l).into_owned()).collect();
        assert_eq!(restored.join(" "), LONG);
        assert_eq!(lines[0], visual(&rows[0]));
    }

    #[test]
    fn punctuation_stays_with_its_own_wrapped_line() {
        let rows = vec!["مرحبا بك في التطبيق،".to_string(), "استمتع بالبث.".to_string()];
        let shown = visual_rows(&rows);
        let lines: Vec<&str> = shown.split('\n').collect();
        assert!(lines[0].ends_with("مرحبا") && lines[0].contains('،'));
        assert!(lines[1].ends_with("استمتع") && lines[1].contains('.'));
    }

    #[test]
    fn narrow_width_wraps_into_rows_that_restore_to_the_original() {
        let mut measure = |s: &str| s.chars().count() as f32 * 8.0;
        let rows = wrap_logical(LONG, 120.0, &mut measure);
        assert!(rows.len() > 2);
        assert!(rows.iter().all(|r| r.chars().count() as f32 * 8.0 <= 120.0 || !r.contains(' ')));
        assert_eq!(rows.join(" "), LONG);
        let shown = visual_rows(&rows);
        let restored: Vec<String> = shown.split('\n').map(|l| visual(l).into_owned()).collect();
        assert_eq!(restored.join(" "), LONG);
    }

    #[test]
    fn explicit_newlines_start_new_rows() {
        let mut measure = |s: &str| s.chars().count() as f32;
        assert_eq!(wrap_logical("أ ب\nج", 100.0, &mut measure), ["أ ب", "ج"]);
    }

    #[test]
    fn detects_rtl_text() {
        assert!(is_rtl_text("مرحبا"));
        assert!(!is_rtl_text("hello"));
    }
}
