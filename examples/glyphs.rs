use zap_stream_app::theme;

fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "glyphs",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(Glyphs))
        }),
    );
}

struct Glyphs;

impl eframe::App for Glyphs {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        for s in ["𝐃eve", "kasynhunsaker 🫶", "🖐️", "日本語 ★ ✓ ∑ ♥ ⚡", "Привет مرحبا"] {
            ui.label(egui::RichText::new(s).size(28.0));
        }
    }
}
