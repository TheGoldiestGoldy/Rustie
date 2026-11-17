mod studio;

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--smoke-audio") {
        studio::smoke_audio();
        return Ok(());
    }
    let native = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Slate Studio | Native Linux DAW")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([880.0, 580.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Slate Studio",
        native,
        Box::new(|cc| Ok(Box::new(studio::Studio::new(cc)))),
    )
}
