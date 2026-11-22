// SPDX-License-Identifier: GPL-3.0-or-later
#![windows_subsystem = "windows"]

mod app;
mod bin_store;
mod organizer;
mod theme;
mod ui_bin;
mod ui_browse;
mod ui_organize;
mod ui_update;
mod updater;

fn main() -> eframe::Result {
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok().map(|i| {
        let r = i.to_rgba8();
        eframe::egui::IconData { width: r.width(), height: r.height(), rgba: r.into_raw() }
    });
    let mut vp = eframe::egui::ViewportBuilder::default()
        .with_title("Image Cleaner")
        .with_inner_size([1360.0, 840.0])
        .with_min_inner_size([960.0, 600.0]);
    if let Some(i) = icon {
        vp = vp.with_icon(i);
    }
    let opts = eframe::NativeOptions { viewport: vp, centered: true, ..Default::default() };
    eframe::run_native(
        "Image Cleaner",
        opts,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
