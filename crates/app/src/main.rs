#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::collapsible_match)]

mod app;
mod cli;
mod format;
mod i18n;
mod icons;
#[cfg(target_os = "macos")]
mod macos_open;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Some(code) = cli::maybe_run_from_args() {
        std::process::exit(code);
    }

    let launch_paths: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .filter(|a| !a.to_string_lossy().starts_with('-'))
        .map(std::path::PathBuf::from)
        .collect();

    let icon = load_icon(include_bytes!("../assets/app-icon.png"));
    let quiet = std::env::var_os("IMAGEOPTIM_QUIET").is_some();
    #[cfg(not(target_os = "macos"))]
    let _ = quiet;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ImageOptim-Rust")
        .with_inner_size([549.0, 297.0])
        .with_min_inner_size([411.0, 110.0])
        .with_max_inner_size([1200.0, 2000.0])
        .with_active(!quiet)
        .with_icon(icon);
    // Upstream frameAutosaveName="MainWindow": restore size and position.
    // QA input runs park the window in a corner so it never overlaps work.
    if std::env::var_os("IMAGEOPTIM_QA_INPUT").is_some() {
        viewport = viewport.with_position([1900.0, 1000.0]);
    } else if let Some([x, y, w, h]) = app::saved_main_window() {
        viewport = viewport.with_inner_size([w, h]).with_position([x, y]);
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if quiet {
        // QA/screenshot mode: keep the window visible but never activate the
        // app, so captures do not disturb whatever the user is doing.
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_activation_policy(ActivationPolicy::Prohibited);
            builder.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "ImageOptim-Rust",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(egui::ThemePreference::System);
            style(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, launch_paths.clone())))
        }),
    )
}

fn load_icon(png: &[u8]) -> egui::IconData {
    let img = image::load_from_memory(png).expect("app icon").to_rgba8();
    egui::IconData {
        rgba: img.as_raw().clone(),
        width: img.width(),
        height: img.height(),
    }
}

fn style(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};
    ctx.style_mut_of(egui::Theme::Light, |s| {
        // The original window is pure white with #F2F2F2 alternating rows.
        s.visuals.panel_fill = egui::Color32::WHITE;
        s.visuals.faint_bg_color = egui::Color32::from_gray(242);
    });
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
            (
                TextStyle::Heading,
                FontId::new(17.0, FontFamily::Proportional),
            ),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.visuals.window_corner_radius = egui::CornerRadius::same(10);
    });
}
