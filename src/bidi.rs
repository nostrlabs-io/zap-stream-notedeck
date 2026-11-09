use std::borrow::Cow;
use unicode_bidi::{BidiClass, BidiInfo, Level};
use unicode_bidi_mirroring::get_mirrored;

struct Token {
    start: usize,
    end: usize,
    level: Level,
}

fn is_rtl_letter(c: char) -> bool {
    (c.is_alphanumeric()
        && matches!(unicode_bidi::bidi_class(c), BidiClass::R | BidiClass::AL))
        || unicode_bidi::bidi_class(c) == BidiClass::NSM
}

fn tokens(text: &str, levels: &[Level], start: usize, end: usize) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut last_letter = false;
    for (i, c) in text[start..end].char_indices() {
        let at = start + i;
        let level = levels[at];
        let letter = level.is_rtl() && is_rtl_letter(c);
        match out.last_mut() {
            Some(t) if letter && last_letter && t.level == level => t.end = at + c.len_utf8(),
            _ => out.push(Token {
                start: at,
                end: at + c.len_utf8(),
                level,
            }),
        }
        last_letter = letter;
    }
    out
}

fn push_token(out: &mut String, text: &str, token: &Token) {
    let slice: String = text[token.start..token.end]
        .chars()
        .filter(|c| !is_isolate(*c))
        .collect();
    if token.level.is_rtl() {
        out.extend(slice.chars().map(|c| {
            if is_rtl_letter(c) {
                c
            } else {
                get_mirrored(c).unwrap_or(c)
            }
        }));
    } else {
        out.push_str(&slice);
    }
}

const LRI: char = '\u{2066}';
const PDI: char = '\u{2069}';

fn is_isolate(c: char) -> bool {
    c == LRI || c == PDI
}

fn has_strong_rtl(word: &str) -> bool {
    word.chars().any(|c| {
        matches!(unicode_bidi::bidi_class(c), unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL)
    })
}

fn isolate_ltr_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for (li, line) in text.split('\n').enumerate() {
        if li > 0 {
            out.push('\n');
        }
        let mut open = false;
        let mut pending_space = String::new();
        for (wi, word) in line.split(' ').enumerate() {
            let ltr = !word.is_empty()
                && !has_strong_rtl(word)
                && word.chars().any(|c| c.is_alphanumeric());
            if wi > 0 {
                if ltr && open {
                    out.push_str(&pending_space);
                } else {
                    if open {
                        out.push(PDI);
                        open = false;
                    }
                    out.push_str(&pending_space);
                }
            }
            pending_space = " ".to_string();
            if ltr && !open {
                out.push(LRI);
                open = true;
            } else if !ltr && open {
                out.push(PDI);
                open = false;
            }
            out.push_str(word);
        }
        if open {
            out.push(PDI);
        }
    }
    out
}

pub fn visual(text: &str) -> Cow<'_, str> {
    if text.is_empty() || text.is_ascii() {
        return Cow::Borrowed(text);
    }
    if !BidiInfo::new(text, None).has_rtl() {
        return Cow::Borrowed(text);
    }
    let marked = isolate_ltr_runs(text);
    let text = marked.as_str();
    let info = BidiInfo::new(text, None);
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

pub fn wrapped_text(ctx: &egui::Context, text: &str, font: egui::FontId, width: f32) -> String {
    if crate::i18n::is_rtl() && is_rtl_text(text) && width.is_finite() && width > 0.0 {
        wrap_rows(ctx, text, font, width)
    } else {
        text.to_string()
    }
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

    const CHAT: &str = "مرحبا بالجميع في هذا البث المباشر الرائع، أتمنى لكم وقتا ممتعا ومفيدا مع الجميع اليوم";
    const TITLE: &str = "بث مباشر لتجربة لعبة المغامرات الجديدة مع المجتمع";

    fn restores(text: &str, width: f32) -> usize {
        let mut measure = |s: &str| s.chars().count() as f32 * 7.5;
        let rows = wrap_logical(text, width, &mut measure);
        let shown = visual_rows(&rows);
        let restored: Vec<String> = shown.split('\n').map(|l| visual(l).into_owned()).collect();
        assert_eq!(restored.join(" "), text);
        rows.len()
    }

    #[test]
    fn long_chat_message_wrapped_at_300px_restores_in_logical_order() {
        assert!(restores(CHAT, 300.0) > 1);
    }

    #[test]
    fn long_title_wrapped_at_300px_restores_in_logical_order() {
        assert!(restores(TITLE, 300.0) > 1);
    }

    #[test]
    fn numbers_and_npubs_stay_intact_and_ltr_in_arabic() {
        let npub = "npub1qqqsyqcyq5rqwzqfpg9scrgwpugpzysnzs23v9ccrydpk8qarc0sx3k0mf";
        let text = format!("أرسل 2,100 sats إلى {npub} الآن");
        let shown = visual(&text);
        assert!(shown.contains("2,100"));
        assert!(shown.contains("sats"));
        assert!(shown.contains(npub));
        let pos = |s: &str| shown.find(s).unwrap();
        assert!(pos("2,100 sats") < shown.len());
        assert!(!shown.contains(LRI) && !shown.contains(PDI));
    }

    #[test]
    fn wallet_paragraph_keeps_full_stop_at_line_end_at_rendered_width() {
        let text = "الصق سلسلة ربط محفظة نوستر (Nostr Wallet Connect) لإرسال \"زابس\" من خلال هذا التطبيق.";
        let mut measure = |s: &str| s.chars().count() as f32 * 7.0;
        let rows = wrap_logical(text, 520.0, &mut measure);
        assert!(rows.len() >= 2);
        assert_eq!(rows.join(" "), text);
        let last = rows.last().unwrap();
        assert!(last.ends_with('.'));
        let shown = visual_rows(&rows);
        let last_shown = shown.split('\n').last().unwrap();
        assert_eq!(last_shown, ".التطبيق هذا");
    }

    #[test]
    fn trailing_full_stop_goes_to_the_visual_left_end_of_the_row() {
        assert_eq!(visual("هذا التطبيق."), ".التطبيق هذا");
    }

    #[test]
    fn question_mark_ends_at_the_visual_left() {
        assert_eq!(visual("هل أنت مستخدم؟"), "؟مستخدم أنت هل");
    }

    #[test]
    fn leading_quote_goes_to_the_visual_right_and_is_mirrored_pair_kept() {
        assert_eq!(visual("\"زابس\" من"), "من \"زابس\"");
    }

    #[test]
    fn latin_run_with_brackets_stays_intact_and_brackets_face_correctly() {
        let shown = visual("نوستر (Nostr Wallet Connect) لإرسال");
        assert!(shown.contains("(Nostr Wallet Connect)"));
        let ws: Vec<&str> = shown.split(' ').collect();
        assert_eq!(ws.first(), Some(&"لإرسال"));
        assert_eq!(ws.last(), Some(&"نوستر"));
    }

    #[test]
    fn brackets_around_arabic_are_mirrored() {
        assert_eq!(visual("(مرحبا) يا"), "يا (مرحبا)");
    }

    #[test]
    fn player_time_label_is_an_ltr_run_inside_arabic() {
        let shown = visual("المدة 1:23:45 متبقية");
        assert!(shown.contains("1:23:45"));
        let ws: Vec<&str> = shown.split(' ').collect();
        assert_eq!(ws, ["متبقية", "1:23:45", "المدة"]);
    }

    #[test]
    fn detects_rtl_text() {
        assert!(is_rtl_text("مرحبا"));
        assert!(!is_rtl_text("hello"));
    }
}
