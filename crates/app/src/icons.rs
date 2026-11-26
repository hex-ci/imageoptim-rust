use eframe::egui;
use egui::TextureHandle;

pub struct Icons {
    wait: TextureHandle,
    progress: TextureHandle,
    ok: TextureHandle,
    noopt: TextureHandle,
    err: TextureHandle,
    reveal: TextureHandle,
}

impl Icons {
    pub fn load(ctx: &egui::Context) -> Self {
        fn tex(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
            let img = image::load_from_memory(bytes)
                .expect("bundled status icon")
                .to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
        }
        Self {
            wait: tex(ctx, "status-wait", include_bytes!("../assets/wait.png")),
            progress: tex(
                ctx,
                "status-progress",
                include_bytes!("../assets/progress.png"),
            ),
            ok: tex(ctx, "status-ok", include_bytes!("../assets/ok.png")),
            noopt: tex(ctx, "status-noopt", include_bytes!("../assets/noopt.png")),
            err: tex(ctx, "status-err", include_bytes!("../assets/err.png")),
            reveal: tex(ctx, "reveal", include_bytes!("../assets/reveal.png")),
        }
    }

    pub fn reveal(&self) -> &TextureHandle {
        &self.reveal
    }

    pub fn get(&self, name: &str) -> &TextureHandle {
        match name {
            "ok" => &self.ok,
            "noopt" => &self.noopt,
            "progress" => &self.progress,
            "err" => &self.err,
            _ => &self.wait,
        }
    }
}
