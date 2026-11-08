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

    #[test]
    fn detects_rtl_text() {
        assert!(is_rtl_text("مرحبا"));
        assert!(!is_rtl_text("hello"));
    }
}
