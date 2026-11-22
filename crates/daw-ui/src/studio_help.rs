// Lightweight, skippable first-run tour. No fabricated functionality.
impl Studio {
    fn tutorial_window(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("Welcome to Rusty · Quick start")
            .id(egui::Id::new("rustie_tutorial"))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false).collapsible(false)
            .show(ctx, |ui| {
                ui.set_max_width(445.0);
                for (title, body) in [
                    ("Listen", "Press Space to play or stop. Click the ruler to seek. Use Click in the transport for a metronome."),
                    ("Edit", "Select a clip to edit it below the timeline. Right-click for track and clip actions. Tab switches between Arrangement and Session."),
                    ("Add devices", "Choose a track, then an effect in the Library. F3 opens its chain. Choose Plug-ins to inspect a bundle and assign an instrument or effect."),
                    ("Record and save", "Choose an input in Devices → Audio / MIDI. Select the target track and press REC. Stop finishes the take. Ctrl+S saves the project."),
                ] {
                    ui.strong(title);
                    ui.label(body);
                    ui.add_space(4.0);
                }
                if ui.button("Skip tutorial").on_hover_text("Dismiss this guide. Reopen it with Help.").clicked() { close = true; }
            });
        if close { self.dismiss_tutorial(); }
    }
}
