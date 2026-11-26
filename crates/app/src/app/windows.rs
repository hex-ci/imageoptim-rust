use eframe::egui;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, UiBuilder,
    pos2, vec2,
};

use super::App;
use super::menus::REPO_URL;
use super::table::paint_text;
use crate::i18n;

impl App {
    pub(super) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.show_prefs {
            return;
        }
        let mut builder = egui::ViewportBuilder::default()
            .with_title(i18n::t_app("ImageOptim Preferences", "ImageOptim 首选项"))
            .with_inner_size([663.0, 376.0])
            .with_min_inner_size([663.0, 361.0])
            .with_max_inner_size([790.0, 371.0])
            .with_resizable(true);
        if let Some([x, y, w, h]) = self.prefs_saved {
            builder = builder.with_inner_size([w, h]).with_position([x, y]);
        }
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("prefs-window"),
            builder,
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    self.show_prefs = false;
                }
                self.prefs_live = ui.ctx().input(|i| i.viewport().outer_rect);
                self.prefs_inner = ui.ctx().input(|i| i.viewport().inner_rect);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let full = ui.max_rect();
                        ui.painter().rect_filled(
                            full,
                            CornerRadius::ZERO,
                            ui.visuals().window_fill,
                        );
                        let content = Rect::from_min_size(
                            pos2(full.left() + 23.0, full.top() + 38.0),
                            vec2(full.width() - 46.0, full.height() - 62.0),
                        );

                        let labels = [
                            i18n::t("General", "常规"),
                            i18n::t("Quality", "质量"),
                            i18n::t("Optimization speed", "优化速度"),
                        ];
                        let galleys: Vec<_> = labels
                            .iter()
                            .map(|l| {
                                ui.painter().layout_no_wrap(
                                    (*l).to_owned(),
                                    FontId::proportional(12.0),
                                    ui.visuals().text_color(),
                                )
                            })
                            .collect();
                        let seg_w: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
                        let total: f32 = seg_w.iter().sum();
                        let bar = Rect::from_min_size(
                            pos2(full.center().x - total / 2.0, full.top() + 8.0),
                            vec2(total, 22.0),
                        );
                        ui.painter().rect_filled(
                            bar,
                            6.0,
                            if ui.visuals().dark_mode {
                                Color32::from_gray(60)
                            } else {
                                Color32::from_gray(230)
                            },
                        );
                        let mut x = bar.left();
                        for (i, galley) in galleys.iter().enumerate() {
                            let seg =
                                Rect::from_min_size(pos2(x, bar.top()), vec2(seg_w[i], 22.0));
                            if i == self.prefs_tab {
                                ui.painter().rect_filled(
                                    seg.shrink(1.0),
                                    5.0,
                                    if ui.visuals().dark_mode {
                                        Color32::from_gray(90)
                                    } else {
                                        ui.visuals().extreme_bg_color
                                    },
                                );
                                ui.painter().rect_stroke(
                                    seg.shrink(1.0),
                                    5.0,
                                    Stroke::new(
                                        1.0,
                                        if ui.visuals().dark_mode {
                                            Color32::from_gray(120)
                                        } else {
                                            Color32::from_gray(200)
                                        },
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                            } else if i + 1 < labels.len() && i + 1 != self.prefs_tab {
                                ui.painter().vline(
                                    seg.right(),
                                    egui::Rangef::new(bar.top() + 5.0, bar.bottom() - 5.0),
                                    Stroke::new(
                                        1.0,
                                        if ui.visuals().dark_mode {
                                            Color32::from_gray(90)
                                        } else {
                                            Color32::from_gray(200)
                                        },
                                    ),
                                );
                            }
                            ui.painter().galley(
                                pos2(
                                    seg.center().x - galley.size().x / 2.0,
                                    seg.center().y - galley.size().y / 2.0,
                                ),
                                galley.clone(),
                                ui.visuals().text_color(),
                            );
                            let resp = ui.interact(
                                seg,
                                ui.scope_id().with(("prefs-tab", i)),
                                Sense::click(),
                            );
                            if resp.clicked() {
                                self.prefs_tab = i;
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            x += seg_w[i];
                        }
                        ui.painter().hline(
                            full.x_range(),
                            bar.bottom() + 0.5,
                            Stroke::new(1.0, Color32::from_gray(225)),
                        );

                        match self.prefs_tab {
                            0 => self.prefs_general(ui, content),
                            1 => self.prefs_quality(ui, content),
                            _ => self.prefs_speed(ui, content),
                        }

                        if self.guetzli_warn_pending {
                            ui.painter().rect_filled(
                                full,
                                CornerRadius::ZERO,
                                Color32::from_black_alpha(100),
                            );
                            let sheet =
                                Rect::from_center_size(full.center(), vec2(420.0, 150.0));
                            ui.painter().rect_filled(
                                sheet,
                                8.0,
                                ui.visuals().window_fill,
                            );
                            ui.painter().rect_stroke(
                                sheet,
                                8.0,
                                Stroke::new(1.0, Color32::from_gray(120)),
                                egui::StrokeKind::Inside,
                            );
                            paint_text(
                                ui,
                                Rect::from_min_size(
                                    pos2(sheet.left() + 20.0, sheet.top() + 16.0),
                                    vec2(380.0, 20.0),
                                ),
                                i18n::t("Guetzli is very slow", "Guetzli 非常慢"),
                                FontId::proportional(13.0),
                                ui.visuals().text_color(),
                                Align2::LEFT_CENTER,
                            );
                            ui.scope_builder(
                                UiBuilder::new().max_rect(Rect::from_min_size(
                                    pos2(sheet.left() + 20.0, sheet.top() + 44.0),
                                    vec2(380.0, 60.0),
                                )),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(i18n::t(
                                                "It can take up to 30 minutes per image. Your system may be unresponsive while Guetzli is running.",
                                                "每张图片可能耗时最多 30 分钟。Guetzli 运行期间系统可能无响应。",
                                            ))
                                            .size(11.0),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                            let ok = ui.put(
                                Rect::from_min_size(
                                    pos2(sheet.right() - 90.0, sheet.bottom() - 38.0),
                                    vec2(70.0, 24.0),
                                ),
                                egui::Button::new("OK"),
                            );
                            if ok.clicked() {
                                self.guetzli_warn_pending = false;
                            }
                        }
                    });
            },
        );
        let json = serde_json::to_string(&self.settings).unwrap_or_default();
        if json != self.last_settings_json {
            self.last_settings_json = json;
            if self.settings.save().is_err() {
                log::warn!("could not save settings");
            }
        }
    }

    fn prefs_general(&mut self, ui: &mut egui::Ui, content: Rect) {
        let bg = ui.visuals().window_fill;
        let right_w = content.width() - 157.0;
        let enable = Rect::from_min_size(
            pos2(content.left() + 9.0, content.top()),
            vec2(131.0, 279.0),
        );
        group_box(ui, enable, i18n::t("Enable", "启用"), bg);
        let tools: [(&str, f32, &mut bool); 10] = [
            ("Zopfli", 29.0, &mut self.settings.zopfli_enabled),
            ("PNGOUT", 49.0, &mut self.settings.pngout_enabled),
            ("OxiPNG", 69.0, &mut self.settings.oxipng_enabled),
            ("AdvPNG", 89.0, &mut self.settings.advpng_enabled),
            ("PNGCrush", 109.0, &mut self.settings.pngcrush_enabled),
            ("JPEGOptim", 129.0, &mut self.settings.jpegoptim_enabled),
            ("Jpegtran", 149.0, &mut self.settings.jpegtran_enabled),
            ("Gifsicle", 189.0, &mut self.settings.gifsicle_enabled),
            ("SVGO", 209.0, &mut self.settings.svgo_enabled),
            ("svgcleaner", 229.0, &mut self.settings.svgcleaner_enabled),
        ];
        for (name, dy, value) in tools {
            let rect = Rect::from_min_size(
                pos2(enable.left() + 18.0, enable.top() + dy),
                vec2(enable.width() - 24.0, 18.0),
            );
            check_at(ui, rect, value, name);
        }
        let grect = Rect::from_min_size(
            pos2(enable.left() + 18.0, enable.top() + 169.0),
            vec2(enable.width() - 24.0, 18.0),
        );
        let before = self.settings.guetzli_enabled;
        let mut g = before;
        check_at(ui, grect, &mut g, "Guetzli");
        if g != before {
            self.set_guetzli(g);
        }
        ui.interact(grect, ui.scope_id().with("guetzli-tip"), Sense::hover())
            .on_hover_text(i18n::t(
                "Guetzli always strips JPEG metadata",
                "Guetzli 总是剔除 JPEG 元数据",
            ));

        let meta = Rect::from_min_size(
            pos2(content.left() + 148.0, content.top()),
            vec2(right_w, 180.0),
        );
        group_box(
            ui,
            meta,
            i18n::t("Metadata and color profiles", "元数据和颜色配置文件"),
            bg,
        );
        check_at(
            ui,
            Rect::from_min_size(
                pos2(meta.left() + 18.0, meta.top() + 18.0),
                vec2(right_w - 40.0, 18.0),
            ),
            &mut self.settings.strip_png_metadata,
            i18n::t(
                "Strip PNG metadata (gamma, color profiles, optional chunks)",
                "剔除 PNG 元数据（伽马值、颜色配置、可选区块）",
            ),
        );
        note_at(
            ui,
            pos2(meta.left() + 36.0, meta.top() + 42.0),
            i18n::t(
                "Web browsers require gamma chunks to be removed",
                "Web 浏览器需要的伽马值区块将被去除",
            ),
        );
        check_at(
            ui,
            Rect::from_min_size(
                pos2(meta.left() + 18.0, meta.top() + 66.0),
                vec2(right_w - 40.0, 18.0),
            ),
            &mut self.settings.strip_jpeg_metadata,
            i18n::t(
                "Strip JPEG metadata (EXIF, color profiles, GPS, rotation, etc.)",
                "剔除 JPEG 元数据（EXIF、颜色配置、GPS 位置、旋转方向等）",
            ),
        );
        if !self.settings.strip_jpeg_metadata && self.settings.guetzli_enabled {
            self.settings.guetzli_enabled = false;
            self.strip_jpeg_before_guetzli = None;
        }
        note_at(
            ui,
            pos2(meta.left() + 36.0, meta.top() + 90.0),
            i18n::t(
                "Not recommended if you rely on embedded copyright information",
                "不推荐，如果你依赖嵌入式的版权信息",
            ),
        );

        let writing = Rect::from_min_size(
            pos2(content.left() + 148.0, content.top() + 188.0),
            vec2(right_w, 93.0),
        );
        group_box(
            ui,
            writing,
            i18n::t("Writing files to disk", "将文件写入磁盘"),
            bg,
        );
        check_at(
            ui,
            Rect::from_min_size(
                pos2(writing.left() + 18.0, writing.top() + 18.0),
                vec2(right_w - 40.0, 18.0),
            ),
            &mut self.settings.preserve_permissions,
            i18n::t(
                "Preserve file permissions, attributes and hardlinks",
                "保留文件的权限、属性和硬链接",
            ),
        );
        note_at(
            ui,
            pos2(writing.left() + 36.0, writing.top() + 42.0),
            i18n::t(
                "Saving to network drives is faster when permissions are not preserved",
                "不保留权限时保存到网络驱动器会更快",
            ),
        );
        check_at(
            ui,
            Rect::from_min_size(
                pos2(writing.left() + 18.0, writing.top() + 58.0),
                vec2(right_w - 40.0, 18.0),
            ),
            &mut self.settings.preserve_dates,
            i18n::t(
                "Preserve file creation and modification dates",
                "保留文件的创建和修改日期",
            ),
        );
        help_button(ui, content, "general");
    }

    fn prefs_quality(&mut self, ui: &mut egui::Ui, content: Rect) {
        let lossy = self.settings.lossy_enabled;
        check_at(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 118.0, content.top() + 12.0),
                vec2(171.0, 18.0),
            ),
            &mut self.settings.lossy_enabled,
            i18n::t("Enable lossy minification", "启用有损压缩"),
        );
        note_at(
            ui,
            pos2(content.left() + 136.0, content.top() + 36.0),
            i18n::t(
                "Makes files much smaller, but may change how images look",
                "使文件更小，但文件的观感可能产生变化",
            ),
        );

        let jpeg_on = lossy && self.settings.jpegoptim_enabled;
        let label_color = |ui: &egui::Ui, on: bool| {
            if on {
                ui.visuals().text_color()
            } else {
                ui.visuals().weak_text_color()
            }
        };

        paint_text(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 33.0, content.top() + 71.0),
                vec2(81.0, 17.0),
            ),
            i18n::t("JPEG quality", "JPEG 质量"),
            FontId::proportional(11.0),
            label_color(ui, lossy),
            Align2::RIGHT_CENTER,
        );
        tick_slider(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 118.0, content.top() + 75.0),
                vec2(445.0, 22.0),
            ),
            &mut self.settings.jpeg_quality,
            50..=99,
            25,
            jpeg_on,
        );
        quality_value(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 568.0, content.top() + 75.0),
                vec2(44.0, 22.0),
            ),
            &mut self.settings.jpeg_quality,
            50..=99,
            jpeg_on,
        );
        tick_labels(
            ui,
            &[
                (content.left() + 118.0, "50%", Align2::LEFT_TOP),
                (content.left() + 327.0, "75%", Align2::CENTER_TOP),
                (content.left() + 563.0, "99%", Align2::RIGHT_TOP),
            ],
            content.top() + 107.0,
        );

        paint_text(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 38.0, content.top() + 128.0),
                vec2(76.0, 17.0),
            ),
            i18n::t("PNG quality", "PNG 质量"),
            FontId::proportional(11.0),
            label_color(ui, lossy),
            Align2::RIGHT_CENTER,
        );
        tick_slider(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 118.0, content.top() + 132.0),
                vec2(166.0, 22.0),
            ),
            &mut self.settings.png_quality,
            40..=100,
            7,
            lossy,
        );
        quality_value(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 286.0, content.top() + 132.0),
                vec2(36.0, 22.0),
            ),
            &mut self.settings.png_quality,
            40..=100,
            lossy,
        );
        tick_labels(
            ui,
            &[
                (content.left() + 118.0, "40%", Align2::LEFT_TOP),
                (content.left() + 188.0, "70%", Align2::CENTER_TOP),
                (content.left() + 256.0, "100%", Align2::RIGHT_TOP),
            ],
            content.top() + 164.0,
        );

        paint_text(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 324.0, content.top() + 128.0),
                vec2(69.0, 17.0),
            ),
            i18n::t("GIF quality", "GIF 质量"),
            FontId::proportional(11.0),
            label_color(ui, lossy),
            Align2::RIGHT_CENTER,
        );
        tick_slider(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 397.0, content.top() + 132.0),
                vec2(166.0, 22.0),
            ),
            &mut self.settings.gif_quality,
            40..=100,
            7,
            lossy,
        );
        quality_value(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 568.0, content.top() + 132.0),
                vec2(44.0, 22.0),
            ),
            &mut self.settings.gif_quality,
            40..=100,
            lossy,
        );
        tick_labels(
            ui,
            &[
                (content.left() + 397.0, "40%", Align2::LEFT_TOP),
                (content.left() + 467.0, "70%", Align2::CENTER_TOP),
                (content.left() + 535.0, "100%", Align2::RIGHT_TOP),
            ],
            content.top() + 164.0,
        );
        help_button(ui, content, "jpegoptim");
    }

    fn prefs_speed(&mut self, ui: &mut egui::Ui, content: Rect) {
        paint_text(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 28.0, content.top() + 36.0),
                vec2(113.0, 17.0),
            ),
            i18n::t("Optimization level", "优化级别"),
            FontId::proportional(11.0),
            ui.visuals().text_color(),
            Align2::RIGHT_CENTER,
        );
        tick_slider(
            ui,
            Rect::from_min_size(
                pos2(content.left() + 145.0, content.top() + 41.0),
                vec2(231.0, 22.0),
            ),
            &mut self.settings.level,
            0..=6,
            7,
            true,
        );
        tick_labels(
            ui,
            &[
                (
                    content.left() + 145.0,
                    i18n::t("Fast", "快速"),
                    Align2::LEFT_TOP,
                ),
                (
                    content.left() + 207.0,
                    i18n::t("Normal", "普通"),
                    Align2::CENTER_TOP,
                ),
                (
                    content.left() + 281.0,
                    i18n::t("Extra", "额外"),
                    Align2::CENTER_TOP,
                ),
                (
                    content.left() + 343.0,
                    i18n::t("Insane", "疯狂"),
                    Align2::LEFT_TOP,
                ),
            ],
            content.top() + 72.0,
        );
        help_button(ui, content, "optipng");
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title(i18n::t_app("About ImageOptim", "关于 ImageOptim"))
            .with_inner_size([320.0, 490.0])
            .with_min_inner_size([312.0, 440.0])
            .with_max_inner_size([380.0, 520.0])
            .with_resizable(true);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("about-window"),
            builder,
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    self.show_about = false;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let full = ui.max_rect();
                        ui.painter().rect_filled(
                            full,
                            CornerRadius::ZERO,
                            ui.visuals().window_fill,
                        );
                        let icon = Rect::from_center_size(
                            pos2(full.center().x + 2.0, full.top() + 15.0 + 64.0),
                            vec2(128.0, 128.0),
                        );
                        if let Some(tex) = &self.app_icon {
                            ui.painter().image(
                                tex.id(),
                                icon,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        let ver = format!("v{} (1.9.3)", imageoptim_core::VERSION);
                        paint_text(
                            ui,
                            Rect::from_min_max(
                                pos2(icon.right() + 4.0, icon.bottom() - 16.0),
                                pos2(full.right() - 11.0, icon.bottom()),
                            ),
                            &ver,
                            FontId::proportional(11.0),
                            ui.visuals().text_color(),
                            Align2::LEFT_BOTTOM,
                        );
                        let credits = Rect::from_min_max(
                            pos2(full.left() + 11.0, icon.bottom() + 17.0),
                            pos2(full.right() - 4.0, full.bottom() - 8.0),
                        );
                        ui.scope_builder(UiBuilder::new().max_rect(credits), |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                credits_ui(ui);
                            });
                        });
                    });
            },
        );
    }

    pub(super) fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(preview) = &self.preview else {
            return;
        };
        let mut open = true;
        egui::Window::new(&preview.title)
            .open(&mut open)
            .resizable(true)
            .default_size([640.0, 480.0])
            .show(ctx, |ui| {
                if !preview.info.is_empty() {
                    ui.label(egui::RichText::new(&preview.info).size(11.0));
                }
                ui.label(
                    egui::RichText::new(preview.path.to_string_lossy())
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.separator();
                if let Some(tex) = &preview.texture {
                    let avail = ui.available_size();
                    let size = tex.size_vec2();
                    let scale = (avail.x / size.x).min(avail.y / size.y).min(1.0);
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size * scale.max(0.05)));
                } else if let Some(text) = &preview.text {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text).font(egui::FontId::monospace(11.0)),
                            )
                            .selectable(true),
                        );
                    });
                }
            });
        if !open {
            self.preview = None;
        }
    }
}

fn group_box(ui: &mut egui::Ui, rect: Rect, title: &str, bg: Color32) {
    ui.painter()
        .rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let color = ui.visuals().widgets.noninteractive.bg_stroke.color;
    ui.painter()
        .rect_stroke(rect, 4.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(
        title.to_owned(),
        FontId::proportional(11.0),
        ui.visuals().text_color(),
    );
    let pos = pos2(rect.left() + 9.0, rect.top() - galley.size().y / 2.0);
    ui.painter().rect_filled(
        Rect::from_min_size(pos - vec2(3.0, 0.0), galley.size() + vec2(6.0, 0.0)),
        CornerRadius::ZERO,
        bg,
    );
    ui.painter().galley(pos, galley, ui.visuals().text_color());
}

fn check_at(ui: &mut egui::Ui, rect: Rect, value: &mut bool, label: &str) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(egui::Checkbox::new(value, RichText::new(label).size(11.0)));
        },
    );
}

fn note_at(ui: &mut egui::Ui, pos: egui::Pos2, text: &str) {
    paint_text(
        ui,
        Rect::from_min_size(pos, vec2(400.0, 14.0)),
        text,
        FontId::proportional(9.5),
        if ui.visuals().dark_mode {
            Color32::from_gray(150)
        } else {
            ui.visuals().weak_text_color()
        },
        Align2::LEFT_CENTER,
    );
}

fn tick_slider(
    ui: &mut egui::Ui,
    rect: Rect,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    ticks: usize,
    enabled: bool,
) {
    let slider_rect = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().slider_width = rect.width().max(40.0);
                ui.add_enabled(enabled, egui::Slider::new(value, range).show_value(false))
                    .rect
            },
        )
        .inner;
    let color = ui.visuals().weak_text_color();
    let y = rect.top() + 15.0;
    let handle_r = slider_rect.height() / 2.5;
    let left = slider_rect.left() + handle_r;
    let right = slider_rect.right() - handle_r;
    for i in 0..ticks {
        let x = left + (right - left) * i as f32 / (ticks - 1).max(1) as f32;
        ui.painter()
            .vline(x, egui::Rangef::new(y, y + 2.0), Stroke::new(1.0, color));
    }
}

pub(super) fn open_help(page: &str) {
    let zh = i18n::language().starts_with("zh");
    let (index, prefs, css, icons): (&str, &str, &[u8], [&[u8]; 5]) = if zh {
        (
            include_str!("../../assets/help/zh-Hans/index.html"),
            include_str!("../../assets/help/zh-Hans/prefs.html"),
            include_bytes!("../../assets/help/zh-Hans/res/style.css"),
            [
                include_bytes!("../../assets/help/zh-Hans/res/ok.png"),
                include_bytes!("../../assets/help/zh-Hans/res/noopt.png"),
                include_bytes!("../../assets/help/zh-Hans/res/progress.png"),
                include_bytes!("../../assets/help/zh-Hans/res/wait.png"),
                include_bytes!("../../assets/help/zh-Hans/res/err.png"),
            ],
        )
    } else {
        (
            include_str!("../../assets/help/en/index.html"),
            include_str!("../../assets/help/en/prefs.html"),
            include_bytes!("../../assets/help/en/res/style.css"),
            [
                include_bytes!("../../assets/help/en/res/ok.png"),
                include_bytes!("../../assets/help/en/res/noopt.png"),
                include_bytes!("../../assets/help/en/res/progress.png"),
                include_bytes!("../../assets/help/en/res/wait.png"),
                include_bytes!("../../assets/help/en/res/err.png"),
            ],
        )
    };
    let base = imageoptim_core::cache::scratch_dir()
        .join("help")
        .join(if zh { "zh-Hans" } else { "en" });
    let help = base.join("Help");
    let _ = std::fs::create_dir_all(&help);
    let _ = std::fs::write(help.join("index.html"), index);
    let _ = std::fs::write(help.join("prefs.html"), prefs);
    let _ = std::fs::write(base.join("style.css"), css);
    for (name, data) in ["ok", "noopt", "progress", "wait", "err"].iter().zip(icons) {
        let _ = std::fs::write(base.join(format!("{name}.png")), data);
    }
    let _ = open::that(help.join(page));
}

fn help_button(ui: &mut egui::Ui, content: Rect, anchor: &str) {
    let rect = Rect::from_min_size(
        pos2(content.left() + 582.0, content.top() + 284.0),
        vec2(25.0, 25.0),
    );
    if ui
        .put(
            rect,
            egui::Button::new(RichText::new("?").size(11.0)).frame(false),
        )
        .on_hover_text(i18n::t("Help", "帮助"))
        .clicked()
    {
        open_help(&format!("prefs.html#{anchor}"));
    }
}

fn tick_labels(ui: &egui::Ui, labels: &[(f32, &str, Align2)], y: f32) {
    for (x, text, align) in labels {
        ui.painter().text(
            pos2(*x, y),
            *align,
            *text,
            FontId::proportional(9.0),
            ui.visuals().weak_text_color(),
        );
    }
}

fn quality_value(
    ui: &mut egui::Ui,
    rect: Rect,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    enabled: bool,
) {
    let mut text = format!("{value}%");
    let changed = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(rect.width())
                        .font(FontId::proportional(11.0)),
                )
                .changed()
            },
        )
        .inner;
    if changed && let Ok(v) = text.trim_end_matches('%').trim().parse::<u8>() {
        *value = v.clamp(*range.start(), *range.end());
    }
}

fn credits_ui(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = 2.0;
    let size = 11.0;
    let zh = i18n::is_chinese();
    let t2 = |en: &'static str, cn: &'static str| if zh { cn } else { en };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.hyperlink_to(RichText::new("ImageOptim-Rust").size(size), REPO_URL);
        ui.label(
            RichText::new(t2(
                " is an unofficial Rust reimplementation inspired by ",
                " 是受原版 ",
            ))
            .size(size),
        );
        ui.hyperlink_to(
            RichText::new("ImageOptim").size(size),
            "https://imageoptim.com",
        );
        ui.label(
            RichText::new(t2(
                " (by Kornel Lesiński and contributors), bundling:",
                "（Kornel Lesiński 及贡献者）启发的非官方 Rust 跨平台实现，集成以下工具：",
            ))
            .size(size),
        );
    });
    ui.add_space(6.0);
    let by = t2(" by ", "，作者 ");
    let items: [(&str, &str, &str); 11] = [
        ("OxiPNG", "https://lib.rs/crates/oxipng", "Josh Holmer,"),
        ("pngquant", "https://pngquant.org", "Kornel Lesiński,"),
        (
            "PNGCrush",
            "http://pmt.sourceforge.net/pngcrush/",
            "Glenn Randers-Pehrson,",
        ),
        (
            "MozJPEG",
            "https://github.com/mozilla/mozjpeg",
            "Mozilla and contributors,",
        ),
        (
            "Guetzli+Zopfli",
            "https://github.com/google/zopfli",
            "Jyrki Alakuijala, Lode Vandevenne,",
        ),
        (
            "AdvPNG",
            "http://advancemame.sourceforge.net/doc-advpng.html",
            "Andrea Mazzoleni, Filipe Estima,",
        ),
        (
            "Jpegoptim",
            "http://www.kokkonen.net/tjko/projects.html",
            "Timo Kokkonen,",
        ),
        ("Gifsicle", "http://www.lcdf.org/gifsicle/", "Eddie Kohler,"),
        ("SVGO", "https://github.com/svg/svgo", "Kir Belevich,"),
        (
            "svgcleaner",
            "https://github.com/RazrFalcon/svgcleaner",
            "Evgeniy Reizner,",
        ),
        (
            "PNGOUT",
            "http://www.advsys.net/ken/utils.htm",
            "Ken Silverman.",
        ),
    ];
    for (i, (name, url, author)) in items.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(RichText::new("•  ").size(size));
            if *name == "Guetzli+Zopfli" {
                ui.hyperlink_to(
                    RichText::new("Guetzli").size(size),
                    "https://github.com/google/guetzli",
                );
                ui.label(RichText::new(", ").size(size));
                ui.hyperlink_to(
                    RichText::new("Zopfli").size(size),
                    "https://github.com/google/zopfli",
                );
                ui.label(RichText::new(format!("{by}{author}")).size(size));
                return;
            }
            if i == items.len() - 1 {
                ui.label(RichText::new(t2("and ", "以及 ")).size(size));
            }
            ui.hyperlink_to(RichText::new(*name).size(size), *url);
            let author = if zh {
                author.replace(" and contributors,", " 及贡献者,")
            } else {
                author.to_string()
            };
            ui.label(RichText::new(format!("{by}{author}")).size(size));
        });
    }
    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            RichText::new(t2(
                "ImageOptim-Rust is licensed under the ",
                "ImageOptim-Rust 基于 ",
            ))
            .size(size),
        );
        ui.hyperlink_to(
            RichText::new(t2(
                "GNU General Public License version 3 or later",
                "GNU 通用公共许可证第 3 版或更高版本",
            ))
            .size(size),
            "https://www.gnu.org/licenses/gpl-3.0.html",
        );
        ui.label(
            RichText::new(t2(
                ". Proprietary PNGOUT is not bundled in open-source releases and is used only if supplied by the user.",
                " 开源。闭源免费的 PNGOUT 不随开源版本分发，仅在用户自行提供时自动启用。",
            ))
            .size(size),
        );
    });
}
