use std::path::PathBuf;

use eframe::egui;
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, pos2, vec2,
};
use egui_extras::{Column, TableBuilder};

use super::App;
use super::state::{Col, RowAction, SortKey, col_range, reveal_in_file_manager, reveal_label};
use crate::format::{bytes, percent, savings};
use crate::i18n;

impl App {
    pub(super) fn status_bar(&mut self) -> (String, bool) {
        let mut bytes_total = 0u64;
        let mut optimized_total = 0u64;
        let mut fraction_total = 0.0f64;
        let mut max_fraction = 0.0f64;
        let mut optimized_count = 0usize;
        let mut any_busy = false;
        for row in &self.rows {
            any_busy |= row.busy;
            let bytes = row.original_size;
            let optimized = row.optimized_size.unwrap_or(bytes);
            if bytes > 0 && optimized > 0 && (bytes != optimized || row.done) {
                let fraction = 1.0 - optimized as f64 / bytes as f64;
                max_fraction = max_fraction.max(fraction);
                fraction_total += fraction;
                bytes_total += bytes;
                optimized_total += optimized;
                optimized_count += 1;
            }
        }

        if optimized_count > 0 && bytes_total > 0 {
            let saved_total = 1.0 - optimized_total as f64 / bytes_total as f64;
            let saved_avg = fraction_total / optimized_count as f64;
            if saved_total > 0.001 {
                if saved_total * 0.8 > saved_avg {
                    self.overall_avg = true;
                } else if saved_avg * 0.8 > saved_total {
                    self.overall_avg = false;
                }
                let (fmt, number) = if self.overall_avg {
                    (
                        i18n::t(
                            "Saved {} out of {}. {} overall (up to {} per file)",
                            "节省了 {}（总计 {}）。总体 {} （每个文件最多 {}）",
                        ),
                        saved_total,
                    )
                } else {
                    (
                        i18n::t(
                            "Saved {} out of {}. {} per file on average (up to {})",
                            "节省了 {}（总共{}）。平均每个文件 {}（最多 {}）",
                        ),
                        saved_avg,
                    )
                };
                let text = fmt
                    .replacen("{}", &bytes(bytes_total - optimized_total), 1)
                    .replacen("{}", &bytes(bytes_total), 1)
                    .replacen("{}", &percent(number * 100.0), 1)
                    .replacen("{}", &percent(max_fraction * 100.0), 1);
                return (text, true);
            }
        }

        if self.settings.guetzli_enabled {
            return (
                i18n::t(
                    "Warning: Guetzli tool enabled. Optimizations may take a very long time.",
                    "警告：已启用 Guetzli，优化可能耗时很久。",
                )
                .into(),
                false,
            );
        }
        if let Some(summary) = self.settings.format_summary() {
            return (
                format!(
                    "{} ({summary})",
                    i18n::t("Lossy minification enabled", "微量有损已启用")
                ),
                false,
            );
        }
        if any_busy {
            return (String::new(), false);
        }
        if self.quit_when_done {
            return (
                i18n::t_app(
                    "ImageOptim will quit when optimizations are complete",
                    "ImageOptim 将在优化完成后退出",
                ),
                false,
            );
        }
        (
            i18n::t(
                "Drag and drop image files onto the area above",
                "拖放图像文件到上方区域",
            )
            .into(),
            false,
        )
    }

    pub(super) fn table_ui(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            self.empty_state(ui);
            return;
        }

        ui.spacing_mut().item_spacing.y = 0.0;
        let spacing = ui.spacing().item_spacing.x;
        let inset = 13.0;
        let trailing = 9.0;
        let width = ui.available_width();

        let visible = self.visible_columns();
        let mut widths: Vec<f32> = vec![inset];
        widths.extend(visible.iter().map(|c| self.col_width(*c)));
        widths.push(trailing);

        // The File column soaks up whatever the fixed columns leave, matching NSTableView.
        let fixed: f32 = widths
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 2)
            .map(|(_, w)| *w)
            .sum::<f32>()
            + spacing * (widths.len().saturating_sub(1)) as f32;
        let layout_changed =
            (self.table_width - width).abs() > 0.5 || self.table_columns != widths.len();
        if layout_changed {
            self.table_width = width;
            self.table_columns = widths.len();
            self.col_file = (width - fixed).max(100.0);
            widths[2] = self.col_file;
        }

        let mut header_sort: Option<SortKey> = None;
        let (header_rect, _) = ui.allocate_exact_size(vec2(width, 23.0), Sense::hover());
        ui.painter().rect_filled(
            header_rect,
            CornerRadius::ZERO,
            ui.visuals().extreme_bg_color,
        );
        ui.painter().hline(
            header_rect.x_range(),
            header_rect.bottom() - 0.5,
            Stroke::new(1.0, Color32::from_gray(225)),
        );

        let mut cols: Vec<(&str, Option<SortKey>, Align)> = vec![("", None, Align::Center)];
        for c in &visible {
            cols.push(match c {
                Col::Status => ("", Some(SortKey::Status), Align::Center),
                Col::File => (i18n::t("File", "文件"), Some(SortKey::Name), Align::LEFT),
                Col::Size => (i18n::t("Size", "大小"), None, Align::RIGHT),
                Col::Savings => (i18n::t("Savings", "已节省"), None, Align::RIGHT),
                Col::Original => (i18n::t("Original Size", "原始大小"), None, Align::RIGHT),
                Col::Best => (i18n::t("Best tool", "最好的工具"), None, Align::LEFT),
            });
        }
        cols.push(("", None, Align::Center));

        let mut starts = Vec::with_capacity(widths.len());
        let mut x = header_rect.left();
        for w in &widths {
            starts.push(x);
            x += w + spacing;
        }

        for (i, (title, key, align)) in cols.iter().enumerate() {
            let cell = Rect::from_min_size(
                pos2(starts[i], header_rect.top()),
                vec2(widths[i], header_rect.height()),
            );
            if !title.is_empty() {
                paint_text(
                    ui,
                    cell,
                    title,
                    FontId::proportional(11.0),
                    ui.visuals().text_color(),
                    if *align == Align::RIGHT {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    },
                );
            }
            if let Some(k) = key
                && self.sort.is_some_and(|(sk, _)| sk == *k)
            {
                let asc = self.sort.is_some_and(|(_, a)| a);
                let cx = cell.right() - 7.0;
                let cy = cell.center().y;
                let tri = if asc {
                    vec![
                        pos2(cx - 3.5, cy + 2.0),
                        pos2(cx + 3.5, cy + 2.0),
                        pos2(cx, cy - 2.5),
                    ]
                } else {
                    vec![
                        pos2(cx - 3.5, cy - 2.0),
                        pos2(cx + 3.5, cy - 2.0),
                        pos2(cx, cy + 2.5),
                    ]
                };
                ui.painter().add(egui::Shape::convex_polygon(
                    tri,
                    ui.visuals().weak_text_color(),
                    Stroke::NONE,
                ));
            }
            let mut resp = ui.interact(
                cell,
                ui.scope_id().with(("hdr", i)),
                Sense::click_and_drag(),
            );
            if let Some(k) = key
                && resp.clicked()
            {
                header_sort = Some(*k);
            }
            if resp.drag_stopped()
                && let Some(px) = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x)
            {
                let target = starts
                    .iter()
                    .enumerate()
                    .find(|(j, st)| px >= **st && px <= **st + widths[*j])
                    .map(|(j, _)| j);
                if let Some(j) = target
                    && i != j
                    && i >= 1
                    && i <= visible.len()
                    && j >= 1
                    && j <= visible.len()
                {
                    let a = visible[i - 1];
                    let b = visible[j - 1];
                    self.move_column(a, b);
                }
            }
            if *key == Some(SortKey::Status) {
                resp = resp.on_hover_text(i18n::t("Optimization status", "优化状态"));
            }
            if i >= 1 && i <= visible.len() && visible[i - 1] == Col::Size {
                resp = resp.on_hover_text(i18n::t(
                    "Size of file after optimization",
                    "优化后的文件大小",
                ));
            }
            resp.context_menu(|ui| self.columns_menu(ui));
        }

        let mut resized = false;
        for i in 1..visible.len() {
            let bx = starts[i] + widths[i] + spacing * 0.5;
            let handle = Rect::from_center_size(
                pos2(bx, header_rect.center().y),
                vec2(9.0, header_rect.height()),
            );
            let resp = ui.interact(
                handle,
                ui.scope_id().with(("col-resize", i)),
                Sense::click_and_drag(),
            );
            ui.painter().vline(
                bx,
                header_rect.shrink2(vec2(0.0, 6.0)).y_range(),
                Stroke::new(1.0, Color32::from_gray(224)),
            );
            let flexible = col_range(visible[i - 1]).is_some() || col_range(visible[i]).is_some();
            if flexible && (resp.hovered() || resp.dragged()) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                ui.painter().vline(
                    bx,
                    header_rect.y_range(),
                    Stroke::new(1.0, Color32::from_gray(150)),
                );
            }
            if resp.dragged() {
                let d = resp.drag_delta().x;
                if d != 0.0 {
                    resized |= self.resize_boundary(visible[i - 1], visible[i], d);
                }
            }
        }

        if resized {
            widths = vec![inset];
            widths.extend(visible.iter().map(|c| self.col_width(*c)));
            widths.push(trailing);
        }

        // Alternating row stripes cover both populated rows and trailing empty space.
        {
            let area = ui.available_rect_before_wrap();
            let mut y = area.top();
            let mut index = 0usize;
            while y < area.bottom() {
                if index % 2 == 1 {
                    let bottom = (y + 19.0).min(area.bottom());
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            pos2(area.left() + 10.0, y),
                            pos2(area.right() - 10.0, bottom),
                        ),
                        5.0,
                        ui.visuals().faint_bg_color,
                    );
                }
                y += 19.0;
                index += 1;
            }
        }

        ui.visuals_mut().widgets.noninteractive.bg_stroke = Stroke::NONE;
        ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
        ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
        let saved_hover = ui.visuals().widgets.hovered.bg_fill;
        let saved_selection = ui.visuals().selection.bg_fill;
        ui.visuals_mut().widgets.hovered.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().selection.bg_fill = Color32::TRANSPARENT;

        let mut actions: Vec<RowAction> = Vec::new();
        let num_font = FontId::proportional(13.0);
        let modifiers = ui.input(|i| i.modifiers);
        let row_area = ui.available_rect_before_wrap();
        let drag_pointer = ui.input(|i| (i.pointer.press_origin(), i.pointer.interact_pos()));
        let sel_bg = Color32::from_rgb(0, 122, 255);
        let row_text = |selected: bool, ui: &egui::Ui| {
            if selected {
                Color32::WHITE
            } else {
                ui.visuals().text_color()
            }
        };

        let mut table = TableBuilder::new(ui)
            .id_salt("files")
            .auto_shrink([false, false])
            .striped(false)
            .sense(Sense::click_and_drag())
            .resizable(false)
            .cell_layout(Layout::left_to_right(Align::Center));
        for w in &widths {
            table = table.column(Column::exact(*w));
        }
        if resized || layout_changed {
            table.reset();
        }

        let mut reorder_preview: Option<(usize, usize)> = None;

        table.body(|body| {
            body.rows(19.0, self.rows.len(), |mut row| {
                let idx = row.index();
                let r = &self.rows[idx];
                let selected = self.selected.contains(&r.id);
                row.set_selected(false);

                row.col(|ui| {
                    if selected {
                        let r = ui.max_rect();
                        let band = Rect::from_min_max(
                            pos2(row_area.left() + 10.0, r.top()),
                            pos2(row_area.right() - 10.0, r.bottom()),
                        );
                        let mut painter = ui.painter().clone();
                        painter.set_clip_rect(band);
                        painter.rect_filled(band, 5.0, sel_bg);
                    }
                });
                for c in &visible {
                    match c {
                        Col::Status => row.col(|ui| {
                            let tex = self.icons.get(r.stop_icon());
                            let rect = ui.max_rect();
                            let icon = Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
                            ui.painter().image(
                                tex.id(),
                                icon,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }),
                        Col::File => row.col(|ui| {
                            let rect = ui.max_rect();
                            let arrow_rect = Rect::from_center_size(
                                pos2(rect.right() - 9.0, rect.center().y),
                                vec2(14.0, 14.0),
                            );
                            let name_rect = Rect::from_min_max(
                                pos2(rect.left() + 6.0, rect.top()),
                                pos2((arrow_rect.left() - 2.0).max(rect.left()), rect.bottom()),
                            );
                            let truncated = paint_text(
                                ui,
                                name_rect,
                                &r.name,
                                FontId::proportional(13.0),
                                row_text(selected, ui),
                                Align2::LEFT_CENTER,
                            );
                            if truncated {
                                ui.interact(
                                    name_rect,
                                    ui.scope_id().with(("expand", r.id)),
                                    Sense::hover(),
                                )
                                .on_hover_text(r.name.clone());
                            }
                            let arrow = ui.interact(
                                arrow_rect,
                                ui.scope_id().with(("reveal", r.id)),
                                Sense::click(),
                            );
                            let base = row_text(selected, ui);
                            let alpha = if arrow.hovered() {
                                255
                            } else if selected {
                                128
                            } else {
                                77
                            };
                            let color = Color32::from_rgba_unmultiplied(
                                base.r(),
                                base.g(),
                                base.b(),
                                alpha,
                            );
                            ui.painter().image(
                                self.icons.reveal().id(),
                                arrow_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                color,
                            );
                            if arrow.clicked() {
                                actions.push(RowAction::Reveal(r.path.clone()));
                            }
                            if arrow.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            arrow.on_hover_text(r.path.to_string_lossy());
                        }),
                        Col::Size => row.col(|ui| {
                            let text = r
                                .optimized_size
                                .map(crate::format::raw_bytes)
                                .unwrap_or_default();
                            paint_text(
                                ui,
                                ui.max_rect(),
                                &text,
                                num_font.clone(),
                                row_text(selected, ui),
                                Align2::RIGHT_CENTER,
                            );
                        }),
                        Col::Savings => row.col(|ui| {
                            let text = if r.optimized {
                                savings(r.percent_saved())
                            } else {
                                String::new()
                            };
                            paint_text(
                                ui,
                                ui.max_rect(),
                                &text,
                                num_font.clone(),
                                row_text(selected, ui),
                                Align2::RIGHT_CENTER,
                            );
                        }),
                        Col::Original => row.col(|ui| {
                            paint_text(
                                ui,
                                ui.max_rect(),
                                &crate::format::raw_bytes(r.original_size),
                                num_font.clone(),
                                row_text(selected, ui),
                                Align2::RIGHT_CENTER,
                            );
                        }),
                        Col::Best => row.col(|ui| {
                            paint_text(
                                ui,
                                ui.max_rect(),
                                r.best_tool.as_deref().unwrap_or(""),
                                num_font.clone(),
                                row_text(selected, ui),
                                Align2::LEFT_CENTER,
                            );
                        }),
                    };
                }
                row.col(|_ui| {});

                let resp = row.response();
                if resp.clicked() {
                    actions.push(RowAction::Select(idx, modifiers));
                }
                if resp.double_clicked() {
                    if selected && self.selected.len() > 1 {
                        for i in self.selected_indices() {
                            actions.push(RowAction::Reveal(self.rows[i].path.clone()));
                        }
                    } else {
                        actions.push(RowAction::Reveal(r.path.clone()));
                    }
                }
                if resp.dragged()
                    && !self.external_drag_started
                    && let Some(cur) = drag_pointer.1
                {
                    if !row_area.expand(6.0).contains(cur) {
                        actions.push(RowAction::BeginExternalDrag(idx));
                    } else {
                        let target = (((cur.y - row_area.top()) / 19.0).floor().max(0.0) as usize)
                            .min(self.rows.len().saturating_sub(1));
                        if target != idx {
                            reorder_preview = Some((idx, target));
                        }
                    }
                }
                if resp.drag_stopped()
                    && !self.external_drag_started
                    && let Some(cur) = drag_pointer.1
                    && row_area.expand(6.0).contains(cur)
                {
                    let target = (((cur.y - row_area.top()) / 19.0).floor().max(0.0) as usize)
                        .min(self.rows.len().saturating_sub(1));
                    if target != idx {
                        actions.push(RowAction::Reorder(idx, target));
                    }
                }
                let row_busy = r.busy;
                let can_revert = r.revert.is_some() && r.done && !r.busy;
                let can_data_url =
                    r.done && !r.failed && r.optimized_size.unwrap_or(r.original_size) <= 100_000;
                let row_path = r.path.clone();
                resp.context_menu(|ui| {
                    if !selected {
                        actions.push(RowAction::SelectContext(idx));
                    }
                    if ui.button(reveal_label()).clicked() {
                        actions.push(RowAction::Reveal(row_path.clone()));
                        ui.close();
                    }
                    if ui.button(i18n::t("Quick Look", "快速查看")).clicked() {
                        actions.push(RowAction::QuickLook);
                        ui.close();
                    }
                    ui.separator();
                    if row_busy {
                        if ui.button(i18n::t("Stop", "停止")).clicked() {
                            actions.push(RowAction::Stop(idx));
                            ui.close();
                        }
                    } else if ui.button(i18n::t("Optimize Again", "再次优化")).clicked() {
                        actions.push(RowAction::Again);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            can_revert,
                            egui::Button::new(i18n::t("Revert to Original", "恢复原状")),
                        )
                        .clicked()
                    {
                        actions.push(RowAction::Revert(idx));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n::t("Copy", "复制")).clicked() {
                        actions.push(RowAction::Copy);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            can_data_url,
                            egui::Button::new(i18n::t("Copy as Data URL", "复制 Data URL")),
                        )
                        .clicked()
                    {
                        actions.push(RowAction::CopyDataUrl);
                        ui.close();
                    }
                    if ui.button(i18n::t("Delete", "删除")).clicked() {
                        actions.push(RowAction::Remove(idx));
                        ui.close();
                    }
                });
                resp.on_hover_text(r.status_text());
            });
        });

        if let Some((from, to)) = reorder_preview {
            let slot = if to > from { to + 1 } else { to };
            let y = (row_area.top() + slot as f32 * 19.0).clamp(row_area.top(), row_area.bottom());
            let left = row_area.left() + 14.0;
            let right = row_area.right() - 10.0;
            ui.painter()
                .hline(egui::Rangef::new(left, right), y, Stroke::new(2.0, sel_bg));
            ui.painter()
                .circle_stroke(pos2(left - 3.0, y), 2.5, Stroke::new(1.5, sel_bg));
        }

        ui.visuals_mut().widgets.hovered.bg_fill = saved_hover;
        ui.visuals_mut().selection.bg_fill = saved_selection;

        if let Some(key) = header_sort {
            let asc = match self.sort {
                Some((k, asc)) if k == key => !asc,
                _ => true,
            };
            self.apply_sort(key, asc);
        }

        let ctx = ui.ctx().clone();
        for action in actions {
            self.handle_row_action(action, &ctx);
        }
    }

    fn handle_row_action(&mut self, action: RowAction, ctx: &egui::Context) {
        match action {
            RowAction::Select(idx, mods) => {
                if idx >= self.rows.len() {
                    return;
                }
                let id = self.rows[idx].id;
                if mods.shift {
                    if let Some(anchor) = self.anchor.and_then(|a| self.row_by_id(a)) {
                        let (a, b) = if anchor < idx {
                            (anchor, idx)
                        } else {
                            (idx, anchor)
                        };
                        self.selected.clear();
                        for i in a..=b {
                            self.selected.insert(self.rows[i].id);
                        }
                    } else {
                        self.selected.clear();
                        self.selected.insert(id);
                    }
                } else if mods.command {
                    if !self.selected.remove(&id) {
                        self.selected.insert(id);
                    }
                    self.anchor = Some(id);
                } else {
                    self.selected.clear();
                    self.selected.insert(id);
                    self.anchor = Some(id);
                }
                self.refresh_quicklook_if_visible(ctx);
            }
            RowAction::SelectContext(idx) => {
                if idx < self.rows.len() {
                    let id = self.rows[idx].id;
                    if !self.selected.contains(&id) {
                        self.selected.clear();
                        self.selected.insert(id);
                        self.anchor = Some(id);
                        self.refresh_quicklook_if_visible(ctx);
                    }
                }
            }
            RowAction::Reveal(path) => reveal_in_file_manager(&path),
            RowAction::Reorder(from, to) => self.apply_reorder(from, to),
            RowAction::BeginExternalDrag(idx) => {
                if !self.external_drag_started && idx < self.rows.len() {
                    self.external_drag_started = true;
                    let id = self.rows[idx].id;
                    if !self.selected.contains(&id) {
                        self.selected.clear();
                        self.selected.insert(id);
                        self.anchor = Some(id);
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let paths: Vec<PathBuf> = self
                            .selected_indices()
                            .into_iter()
                            .map(|i| self.rows[i].path.clone())
                            .collect();
                        let _ = crate::macos_open::begin_file_drag(&paths);
                    }
                    #[cfg(not(target_os = "macos"))]
                    self.copy_paths(ctx);
                }
            }
            RowAction::QuickLook => self.preview_selected(ctx),
            RowAction::Again => self.start_again(false),
            RowAction::Stop(idx) => {
                let targets = self.row_or_selection(idx);
                self.stop(&targets);
            }
            RowAction::Revert(idx) => {
                for i in self.row_or_selection(idx) {
                    if i < self.rows.len() && self.rows[i].revert.is_some() && !self.rows[i].busy {
                        self.revert_row(i);
                    }
                }
            }
            RowAction::Copy => self.copy_paths(ctx),
            RowAction::CopyDataUrl => self.copy_data_urls(ctx),
            RowAction::Remove(idx) => {
                let targets = self.row_or_selection(idx);
                self.remove_rows(&targets, Some("Delete"));
            }
        }
    }

    fn row_or_selection(&self, idx: usize) -> Vec<usize> {
        if idx < self.rows.len() && self.selected.contains(&self.rows[idx].id) {
            self.selected_indices()
        } else {
            vec![idx]
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter();
        let size = (rect.width() / 4.0).min(rect.height() / 1.5);
        let frame =
            egui::Rect::from_center_size(rect.center(), vec2(size.max(40.0), size.max(40.0)));
        let base = ui.visuals().weak_text_color();
        let alpha = if self.drop_hover { 64 } else { 32 };
        let color = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
        let stroke = Stroke::new((size / 32.0).max(2.0).round(), color);
        let radius = (size / 14.0).max(2.0);
        let path = rounded_rect_path(frame, radius);
        painter.extend(egui::Shape::dashed_line_with_offset(
            &path,
            stroke,
            &[size / 10.0],
            &[size / 16.0],
            2.0,
        ));

        let base_width = size / 8.0;
        let base_height = size / 8.0;
        let arrow_width = base_width * 2.0;
        let point_height = base_height * 3.0;
        let shift = size / 8.0;
        let c = rect.center();
        let top = c.y - base_height - shift;
        let bottom = c.y + base_height - shift;
        let tip = c.y + point_height - shift;
        painter.rect_filled(
            Rect::from_min_max(pos2(c.x - base_width, top), pos2(c.x + base_width, bottom)),
            CornerRadius::ZERO,
            color,
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(c.x - arrow_width, bottom),
                pos2(c.x + arrow_width, bottom),
                pos2(c.x, tip),
            ],
            color,
            Stroke::NONE,
        ));
    }

    pub(super) fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(7.0);
            let add = ui.add_sized(
                vec2(30.0, 22.0),
                egui::Button::new(RichText::new("+").size(16.0)).corner_radius(5.0),
            );
            if add.clicked() {
                self.browse_files();
            }
            add.on_hover_text(i18n::t("Add", "添加"));

            ui.add_space(7.0);
            let busy = self.is_busy();
            let gear_w = if busy { 0.0 } else { 24.0 };
            let right_w = 8.0 + 90.0 + 8.0 + gear_w + 8.0 + if busy { 18.0 } else { 0.0 };
            let status_w = (ui.available_width() - right_w).max(40.0);
            let (text, selectable) = self.status_bar();
            let resp = ui
                .allocate_ui_with_layout(
                    vec2(status_w, 14.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(text).size(11.0))
                                .truncate()
                                .halign(Align::LEFT),
                        )
                    },
                )
                .inner;
            if selectable {
                resp.on_hover_text(i18n::t("Savings summary", "节省摘要"));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let alt = ui.input(|i| i.modifiers.alt);
                let title = if alt {
                    i18n::t("Run optimizations again", "再次进行优化处理")
                } else {
                    i18n::t("Again", "再优化")
                };
                let again = ui.add_enabled(
                    !self.rows.is_empty(),
                    egui::Button::new(RichText::new(format!("↻  {title}")).size(12.0))
                        .min_size(vec2(90.0, 22.0)),
                );
                if again.clicked() {
                    self.start_again(alt);
                }
                ui.add_space(4.0);
                if !busy {
                    let gear = ui.add_sized(
                        vec2(22.0, 22.0),
                        egui::Button::new(RichText::new("⚙").size(13.0)).frame(false),
                    );
                    if gear.clicked() {
                        self.show_prefs = true;
                        self.prefs_tab = 0;
                    }
                    gear.on_hover_text(i18n::t("Settings", "设置"));
                }
                if busy {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
    }

    pub(super) fn browse_files(&mut self) {
        let exts = self.settings.extensions();
        if exts.is_empty() {
            self.beep();
            return;
        }
        #[cfg(target_os = "macos")]
        let picked = crate::macos_open::browse_files(&exts);
        #[cfg(not(target_os = "macos"))]
        let picked = rfd::FileDialog::new()
            .add_filter("Images", &exts)
            .pick_files();
        if let Some(paths) = picked {
            self.add_paths(paths);
        }
    }

    pub(super) fn handle_drops(&mut self, ctx: &egui::Context) {
        let (hovered, dropped) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>(),
            )
        });
        self.drop_hover = hovered;
        if !dropped.is_empty() {
            self.add_paths(dropped);
        }
    }
}

fn rounded_rect_path(rect: egui::Rect, radius: f32) -> Vec<egui::Pos2> {
    use std::f32::consts::FRAC_PI_2;
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let centers = [
        (egui::pos2(rect.right() - r, rect.top() + r), -FRAC_PI_2),
        (egui::pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (egui::pos2(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
        (
            egui::pos2(rect.left() + r, rect.top() + r),
            std::f32::consts::PI,
        ),
    ];
    let mut pts = Vec::with_capacity(36);
    for (center, start) in centers {
        for i in 0..=8 {
            let a = start + FRAC_PI_2 * (i as f32 / 8.0);
            pts.push(egui::pos2(center.x + r * a.cos(), center.y + r * a.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

/// Draws a single line of text vertically centred in `rect` and elided at its width.
pub(super) fn paint_text(
    ui: &egui::Ui,
    rect: Rect,
    text: &str,
    font: FontId,
    color: Color32,
    align: Align2,
) -> bool {
    if text.is_empty() {
        return false;
    }
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap = TextWrapping {
        max_width: rect.width().max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let truncated = galley.rows.len() > 1 || galley.size().x > rect.width();
    let pos = align.align_size_within_rect(galley.size(), rect).min;
    ui.painter().galley(pos, galley, color);
    truncated
}

/// Resizes two neighbouring columns by `d` without changing their combined width.
pub(super) fn adjust_pair(
    left: &mut f32,
    right: &mut f32,
    d: f32,
    left_range: (f32, f32),
    right_range: (f32, f32),
) {
    let lo = (left_range.0 - *left).max(*right - right_range.1);
    let hi = (left_range.1 - *left).min(*right - right_range.0);
    if lo <= hi {
        let d = d.clamp(lo, hi);
        *left += d;
        *right -= d;
    }
}
