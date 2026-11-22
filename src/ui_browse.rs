// SPDX-License-Identifier: GPL-3.0-or-later
use crate::app::*;
use crate::organizer;
use crate::ui_organize::suggest_output;
use crate::theme::{self, button, fmt_num, fmt_size, ic, icon, icon_btn, paint_icon, segmented, Palette};
use eframe::egui::{self, epaint, *};
use std::path::{Path, PathBuf};

const UV: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };

fn chip(ui: &mut Ui, p: &Palette, ico: &str, label: &str, on: bool) -> Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), p.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 28.0), Sense::click());
    let fill = if on { p.accent_soft } else if resp.hovered() { p.hover } else { p.widget };
    ui.painter().rect_filled(rect, 14.0, fill);
    let col = if on { p.accent } else { p.text };
    paint_icon(ui.painter(), pos2(rect.left() + 16.0, rect.center().y), Align2::CENTER_CENTER, ico, 13.0, col);
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), col);
    ui.painter().galley(pos2(rect.left() + 29.0, rect.center().y - g.size().y / 2.0), g, col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn spinner(painter: &Painter, c: Pos2, r: f32, col: Color32, t: f64) {
    theme::arc(painter, c, r, (t * 5.0) as f32, 4.2, Stroke::new(2.5, col));
}

impl App {
    // ───────── 상단 앱바 ─────────
    pub fn app_bar(&mut self, ui: &mut Ui, p: &Palette) {
        Panel::top("appbar")
            .exact_size(54.0)
            .frame(Frame::new().fill(p.bar).inner_margin(Margin::symmetric(16, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                    ui.painter().rect_filled(r, 8.0, p.accent);
                    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, ic::PHOTO, 16.0, p.on_accent);
                    ui.label(RichText::new("Image Cleaner").strong().size(16.0));
                    ui.add_space(22.0);
                    let mut t = self.tab;
                    let bin_label = if self.bin_items.is_empty() { "휴지통".to_string() } else { format!("휴지통 ({})", self.bin_items.len()) };
                    segmented(ui, p, &mut t, &[(Tab::Browse, ic::GRID, "탐색"), (Tab::Organize, ic::MERGE, "정리"), (Tab::Bin, ic::DELETE, &bin_label)]);
                    if t == Tab::Bin && self.tab != Tab::Bin {
                        self.bin_dirty = true;
                    }
                    self.tab = t;
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_btn(ui, p, ic::INFO, "정보 및 단축키", false).clicked() {
                            self.about = true;
                        }
                        if icon_btn(ui, p, if self.dark { ic::SUN } else { ic::MOON }, if self.dark { "라이트 테마" } else { "다크 테마" }, false).clicked() {
                            self.dark = !self.dark;
                            theme::apply_theme(ui.ctx(), self.dark);
                            self.save_settings();
                        }
                        if self.tab == Tab::Browse {
                            ui.add_space(6.0);
                            let (rect, _) = ui.allocate_exact_size(vec2(240.0, 32.0), Sense::hover());
                            ui.painter().rect_filled(rect, 16.0, p.widget);
                            paint_icon(ui.painter(), pos2(rect.left() + 18.0, rect.center().y), Align2::CENTER_CENTER, ic::SEARCH, 14.0, p.weak);
                            let te = ui.put(
                                Rect::from_min_max(rect.min + vec2(34.0, 3.0), rect.max - vec2(12.0, 3.0)),
                                TextEdit::singleline(&mut self.filter).hint_text("파일명 검색").frame(Frame::NONE).margin(Margin::ZERO),
                            );
                            if self.focus_search {
                                te.request_focus();
                                self.focus_search = false;
                            }
                        }
                    });
                });
            });
    }

    // ───────── 경로 이동줄 ─────────
    pub fn path_bar(&mut self, ui: &mut Ui, p: &Palette) {
        Panel::top("pathbar")
            .exact_size(46.0)
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.add_enabled_ui(!self.back.is_empty(), |ui| {
                        if icon_btn(ui, p, ic::BACK, "뒤로", false).clicked() {
                            if let Some(d) = self.back.pop() {
                                self.navigate(d, false);
                            }
                        }
                    });
                    let parent = self.selected_dir.as_ref().and_then(|d| d.parent().map(|p| p.to_path_buf()));
                    ui.add_enabled_ui(parent.is_some(), |ui| {
                        if icon_btn(ui, p, ic::UP, "상위 폴더", false).clicked() {
                            if let Some(d) = parent.clone() {
                                self.pick = Some(d);
                            }
                        }
                    });
                    if icon_btn(ui, p, ic::REFRESH, "새로고침 (F5)", false).clicked() {
                        self.kids.clear();
                        self.reload();
                    }
                    if let Some(d) = self.selected_dir.clone() {
                        if icon_btn(ui, p, ic::MERGE, "현재 폴더 정리하기…", false).clicked() {
                            self.quick = Some(QuickOrg { out: suggest_output(&d), src: d, by_year: self.org.by_year, trash_source: self.org.trash_source, suffix_all: self.org.suffix_all });
                        }
                    }
                    ui.add_space(8.0);
                    // 이동 경로(breadcrumb)
                    if let Some(dir) = self.selected_dir.clone() {
                        let mut parts: Vec<PathBuf> = dir.ancestors().map(|a| a.to_path_buf()).collect();
                        parts.reverse();
                        let skip = parts.len().saturating_sub(4);
                        if skip > 0 {
                            ui.label(RichText::new("…").color(p.weak));
                            paint_chev(ui, p);
                        }
                        let n = parts.len();
                        for (i, part) in parts.into_iter().enumerate().skip(skip) {
                            let label = part
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| part.display().to_string().trim_end_matches('\\').to_string());
                            let last = i + 1 == n;
                            let g = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(14.0), p.text);
                            let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 28.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(rect, 6.0, p.hover);
                            }
                            let col = if last { p.text } else { p.weak };
                            let g = ui.painter().layout_no_wrap(label, FontId::proportional(14.0), col);
                            ui.painter().galley(pos2(rect.left() + 8.0, rect.center().y - g.size().y / 2.0), g, col);
                            if resp.clicked() && !last {
                                self.pick = Some(part);
                            }
                            if !last {
                                paint_chev(ui, p);
                            }
                        }
                    } else {
                        ui.label(RichText::new("왼쪽에서 폴더를 선택하거나 폴더를 끌어다 놓으세요").color(p.weak));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add(Slider::new(&mut self.thumb, 90.0..=300.0).show_value(false).trailing_fill(true));
                        paint_icon_inline(ui, p, ic::PHOTO, 17.0);
                        ui.add_space(4.0);
                        paint_icon_inline(ui, p, ic::PHOTO, 11.0);
                        ui.add_space(8.0);
                        if chip(ui, p, ic::FOLDER_OPEN, "하위 폴더 포함", self.recursive).clicked() {
                            self.recursive = !self.recursive;
                            self.reload();
                        }
                        if icon_btn(ui, p, ic::SORT, if self.ascending { "오름차순 (클릭하면 내림차순)" } else { "내림차순 (클릭하면 오름차순)" }, !self.ascending).clicked() {
                            self.ascending = !self.ascending;
                            self.sort_entries();
                        }
                        let cur = match self.sort { Sort::Name => "이름", Sort::Modified => "수정일", Sort::Size => "크기" };
                        egui::ComboBox::from_id_salt("sort").width(90.0).selected_text(cur).show_ui(ui, |ui| {
                            let mut ch = false;
                            ch |= ui.selectable_value(&mut self.sort, Sort::Name, "이름").changed();
                            ch |= ui.selectable_value(&mut self.sort, Sort::Modified, "수정일").changed();
                            ch |= ui.selectable_value(&mut self.sort, Sort::Size, "크기").changed();
                            if ch {
                                self.sort_entries();
                            }
                        });
                    });
                });
            });
    }

    // ───────── 상태바 ─────────
    pub fn status_bar(&mut self, ui: &mut Ui, p: &Palette) {
        Panel::bottom("status")
            .exact_size(28.0)
            .frame(Frame::new().fill(p.bar).inner_margin(Margin::symmetric(14, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let weak = |s: String| RichText::new(s).size(12.5).color(p.weak);
                    if self.tab == Tab::Browse {
                        let n = self.visible().len();
                        let mut s = format!("이미지 {}개", fmt_num(n));
                        if !self.sel.is_empty() {
                            let tot: u64 = self.sel.iter().filter_map(|&i| self.entries.get(i)).map(|e| e.size).sum();
                            s += &format!("   ·   {}개 선택 ({})", self.sel.len(), fmt_size(tot));
                        }
                        ui.label(weak(s));
                    } else if self.tab == Tab::Bin {
                        ui.label(weak(format!("앱 내 휴지통 · {}개", self.bin_items.len())));
                    } else {
                        ui.label(weak(match self.org.state {
                            RunState::Running => self.org.phase.clone(),
                            RunState::Idle => "대기 중".into(),
                        }));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.pending.is_empty() {
                            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            spinner(ui.painter(), r.center(), 5.0, p.accent, ui.input(|i| i.time));
                            ui.label(weak("썸네일 불러오는 중".into()));
                            ui.ctx().request_repaint();
                        }
                    });
                });
            });
    }

    // ───────── 폴더 트리 ─────────
    pub fn sidebar(&mut self, ui: &mut Ui, p: &Palette) {
        Panel::left("side")
            .default_size(270.0)
            .size_range(210.0..=440.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::symmetric(8, 10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("폴더").strong().size(13.0).color(p.weak));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_btn(ui, p, ic::NEW_FOLDER, "폴더 추가", false).clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                let n = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| d.display().to_string());
                                self.roots.insert(0, Root { label: n, path: d.clone(), icon: ic::FOLDER });
                                self.pick = Some(d);
                                self.save_settings();
                            }
                        }
                    });
                });
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let roots: Vec<(String, PathBuf, &'static str)> =
                        self.roots.iter().map(|r| (r.label.clone(), r.path.clone(), r.icon)).collect();
                    let mut first_drive = true;
                    let mut first_fav = true;
                    for (label, path, icon) in roots {
                        if icon == ic::DRIVE && first_drive {
                            first_drive = false;
                            ui.add_space(10.0);
                            ui.label(RichText::new("   내 PC").size(12.0).color(p.weak));
                            ui.add_space(2.0);
                        } else if icon != ic::DRIVE && first_fav {
                            first_fav = false;
                            ui.label(RichText::new("   즐겨찾기").size(12.0).color(p.weak));
                            ui.add_space(2.0);
                        }
                        self.tree_row(ui, p, &path, &label, icon, 0);
                    }
                });
            });
    }

    fn tree_row(&mut self, ui: &mut Ui, p: &Palette, path: &Path, label: &str, ico: &str, depth: usize) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let selected = self.selected_dir.as_deref() == Some(path);
        let open = self.expanded.contains(path);
        let leaf = matches!(self.kids.get(path), Some(k) if k.is_empty());
        if selected && self.scroll_tree {
            ui.scroll_to_rect(rect, Some(Align::Center));
            self.scroll_tree = false;
        }
        if ui.is_rect_visible(rect) {
            if selected {
                ui.painter().rect_filled(rect, 6.0, p.accent_soft);
                ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, 18.0)), 2.0, p.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 6.0, p.hover);
            }
            let x = rect.left() + 8.0 + depth as f32 * 16.0;
            if !leaf {
                paint_icon(ui.painter(), pos2(x + 8.0, rect.center().y), Align2::CENTER_CENTER, if open { ic::CHEV_D } else { ic::CHEV_R }, 10.0, p.weak);
            }
            let fc = if ico == ic::FOLDER { p.warn } else if selected { p.accent } else { p.weak };
            let glyph = if ico == ic::FOLDER && (open || selected) { ic::FOLDER_OPEN } else { ico };
            paint_icon(ui.painter(), pos2(x + 30.0, rect.center().y), Align2::CENTER_CENTER, glyph, 15.0, fc);
            let col = if selected { p.accent } else { p.text };
            let lab = Rect::from_min_max(pos2(x + 46.0, rect.top()), pos2(rect.right() - 6.0, rect.bottom()));
            ui.painter().with_clip_rect(lab).text(pos2(lab.left(), rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), col);
        }
        if resp.clicked() {
            let on_chev = resp.interact_pointer_pos().map(|pp| pp.x < rect.left() + 8.0 + depth as f32 * 16.0 + 18.0).unwrap_or(false);
            if on_chev && !leaf {
                if !self.expanded.remove(path) {
                    self.expanded.insert(path.to_path_buf());
                }
            } else {
                self.pick = Some(path.to_path_buf());
                self.expanded.insert(path.to_path_buf());
            }
        }
        if resp.double_clicked() {
            if !self.expanded.remove(path) {
                self.expanded.insert(path.to_path_buf());
            }
        }
        resp.context_menu(|ui| {
            if ui.button("현재 폴더 정리하기…").clicked() {
                self.quick = Some(QuickOrg {
                    src: path.to_path_buf(),
                    out: suggest_output(path),
                    by_year: self.org.by_year,
                    trash_source: self.org.trash_source,
                    suffix_all: self.org.suffix_all,
                });
                ui.close();
            }
            if ui.button("이 폴더 열기").clicked() {
                self.pick = Some(path.to_path_buf());
                ui.close();
            }
            if ui.button("탐색기에서 열기").clicked() {
                let _ = open::that(path);
                ui.close();
            }
        });
        if self.expanded.contains(path) {
            for k in self.children(path) {
                let n = k.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.tree_row(ui, p, &k, &n, ic::FOLDER, depth + 1);
            }
        }
    }

    // ───────── 썸네일 그리드 ─────────
    pub fn grid(&mut self, ui: &mut Ui, p: &Palette) {
        let vis = self.visible();
        if self.selected_dir.is_none() || vis.is_empty() {
            let (msg, sub, glyph) = if self.selected_dir.is_none() {
                ("폴더를 선택하세요", "왼쪽 트리에서 폴더를 고르거나, 폴더를 이 창으로 끌어다 놓으세요", ic::FOLDER_OPEN)
            } else if self.entries.is_empty() {
                ("이미지가 없습니다", "이 폴더에는 지원되는 이미지 파일이 없어요. '하위 폴더 포함'을 켜 보세요", ic::PHOTO)
            } else {
                ("검색 결과가 없습니다", "다른 검색어를 입력해 보세요", ic::SEARCH)
            };
            let r = ui.max_rect();
            let c = r.center();
            ui.painter().circle_filled(c - vec2(0.0, 50.0), 44.0, p.card);
            paint_icon(ui.painter(), c - vec2(0.0, 50.0), Align2::CENTER_CENTER, glyph, 34.0, p.weak);
            ui.painter().text(c + vec2(0.0, 18.0), Align2::CENTER_CENTER, msg, FontId::proportional(18.0), p.text);
            ui.painter().text(c + vec2(0.0, 46.0), Align2::CENTER_CENTER, sub, FontId::proportional(13.0), p.weak);
            return;
        }
        let cell = self.thumb;
        let cell_h = cell + 44.0;
        let sp = 10.0;
        let cols = (((ui.available_width() - 8.0) / (cell + sp)).floor() as usize).max(1);
        self.cols = cols;
        let rows = vis.len().div_ceil(cols);
        let mut action: Option<(usize, u8)> = None;
        let mut click: Option<(usize, Modifiers)> = None;
        let mods = ui.input(|i| i.modifiers);
        ui.spacing_mut().item_spacing = vec2(sp, sp);
        let mut sa = ScrollArea::vertical().auto_shrink(false);
        if let Some(r) = self.scroll_to.take() {
            sa = sa.vertical_scroll_offset((r as f32 * (cell_h + sp) - 40.0).max(0.0));
        }
        sa.show_rows(ui, cell_h, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    for c in 0..cols {
                        let Some(&idx) = vis.get(r * cols + c) else { break };
                        let (rect, resp) = ui.allocate_exact_size(vec2(cell, cell_h), Sense::click());
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let selected = self.sel.contains(&idx);
                        let e = &self.entries[idx];
                        let fill = if selected { p.accent_soft } else if resp.hovered() { p.hover } else { p.card };
                        ui.painter().rect_filled(rect, 10.0, fill);
                        if selected {
                            ui.painter().rect_stroke(rect, 10.0, Stroke::new(2.0, p.accent), StrokeKind::Inside);
                        }
                        let img_rect = Rect::from_min_size(rect.min, vec2(cell, cell)).shrink(8.0);
                        match self.thumbs.get(&e.path) {
                            Some(Some(t)) => {
                                let s = t.size_vec2();
                                let k = (img_rect.width() / s.x).min(img_rect.height() / s.y);
                                let r = Rect::from_center_size(img_rect.center(), s * k);
                                ui.painter().add(epaint::RectShape::filled(r, 5.0, Color32::WHITE).with_texture(t.id(), UV));
                            }
                            Some(None) => {
                                paint_icon(ui.painter(), img_rect.center(), Align2::CENTER_CENTER, ic::ERROR, 26.0, p.weak);
                            }
                            None => {
                                spinner(ui.painter(), img_rect.center(), 9.0, p.weak, ui.input(|i| i.time));
                                if self.pending.len() < 48 {
                                    if self.pending.insert(e.path.clone()) {
                                        let _ = self.loader.req_tx.send(Req { path: e.path.clone(), kind: Kind::Thumb });
                                    }
                                }
                            }
                        }
                        if selected || resp.hovered() {
                            let b = pos2(rect.left() + 20.0, rect.top() + 20.0);
                            if selected {
                                ui.painter().circle_filled(b, 10.0, p.accent);
                                paint_icon(ui.painter(), b, Align2::CENTER_CENTER, ic::CHECK, 11.0, p.on_accent);
                            } else {
                                ui.painter().circle_stroke(b, 9.0, Stroke::new(1.5, p.weak));
                            }
                        }
                        let lab = Rect::from_min_size(pos2(rect.left() + 10.0, rect.bottom() - 38.0), vec2(cell - 20.0, 34.0));
                        let ext = e.path.extension().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
                        ui.painter().with_clip_rect(lab).text(lab.left_top() + vec2(0.0, 3.0), Align2::LEFT_TOP, &e.name, FontId::proportional(13.0), p.text);
                        ui.painter().with_clip_rect(lab).text(lab.left_top() + vec2(0.0, 20.0), Align2::LEFT_TOP, format!("{ext} · {}", fmt_size(e.size)), FontId::proportional(11.5), p.weak);
                        if resp.clicked() {
                            click = Some((idx, mods));
                        }
                        if resp.double_clicked() {
                            action = Some((idx, 0));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("크게 보기").clicked() { action = Some((idx, 0)); ui.close(); }
                            if ui.button("기본 앱으로 열기").clicked() { action = Some((idx, 1)); ui.close(); }
                            if ui.button("탐색기에서 보기").clicked() { action = Some((idx, 2)); ui.close(); }
                            if ui.button("경로 복사").clicked() { action = Some((idx, 3)); ui.close(); }
                            if ui.button("삭제 (앱 휴지통으로)").clicked() { action = Some((idx, 4)); ui.close(); }
                        });
                    }
                });
            }
        });
        if let Some((idx, m)) = click {
            if m.shift && self.anchor.is_some() {
                let a = vis.iter().position(|&v| Some(v) == self.anchor).unwrap_or(0);
                let b = vis.iter().position(|&v| v == idx).unwrap_or(0);
                self.sel.clear();
                for &v in &vis[a.min(b)..=a.max(b)] {
                    self.sel.insert(v);
                }
                self.set_primary(idx);
            } else if m.command || m.ctrl {
                if !self.sel.remove(&idx) {
                    self.sel.insert(idx);
                    self.set_primary(idx);
                }
                self.anchor = Some(idx);
            } else {
                self.select_only(idx);
            }
        }
        if let Some((idx, a)) = action {
            if !self.sel.contains(&idx) {
                self.select_only(idx);
            }
            let path = self.entries[idx].path.clone();
            match a {
                0 => self.viewer = true,
                1 => {
                    let _ = open::that(&path);
                }
                2 => reveal(&path),
                3 => {
                    ui.ctx().copy_text(path.display().to_string());
                    self.toast(ui.ctx(), "경로를 복사했습니다");
                }
                _ => self.confirm_delete = true,
            }
        }
    }

    // ───────── 우측 인스펙터 ─────────
    pub fn inspector(&mut self, ui: &mut Ui, p: &Palette) {
        Panel::right("inspector")
            .default_size(350.0)
            .size_range(290.0..=600.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                let Some(pi) = self.primary.filter(|&i| i < self.entries.len()) else {
                    let r = ui.max_rect();
                    ui.painter().text(r.center() - vec2(0.0, 16.0), Align2::CENTER_CENTER, ic::INFO, FontId::new(32.0, theme::icons()), p.weak);
                    ui.painter().text(r.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, "이미지를 선택하면\n미리보기와 상세 정보가 표시됩니다", FontId::proportional(13.5), p.weak);
                    return;
                };
                let path = self.entries[pi].path.clone();
                let name = self.entries[pi].name.clone();
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    if self.sel.len() > 1 {
                        let tot: u64 = self.sel.iter().filter_map(|&i| self.entries.get(i)).map(|e| e.size).sum();
                        ui.label(RichText::new(format!("{}개 항목 선택됨", self.sel.len())).strong().size(16.0));
                        ui.label(RichText::new(format!("총 {}", fmt_size(tot))).color(p.weak));
                    } else {
                        ui.label(RichText::new(&name).strong().size(16.0));
                    }
                    // 미리보기 카드
                    let w = ui.available_width();
                    let h = (w * 0.78).clamp(180.0, 340.0);
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                    ui.painter().rect(rect, 10.0, p.card, Stroke::new(1.0, p.border), StrokeKind::Inside);
                    match &self.preview {
                        Some((_, Some(t))) => {
                            let s = t.size_vec2();
                            let inner = rect.shrink(8.0);
                            let k = (inner.width() / s.x).min(inner.height() / s.y);
                            let r = Rect::from_center_size(inner.center(), s * k);
                            ui.painter().add(epaint::RectShape::filled(r, 4.0, Color32::WHITE).with_texture(t.id(), UV));
                        }
                        _ => spinner(ui.painter(), rect.center(), 12.0, p.accent, ui.input(|i| i.time)),
                    }
                    if resp.hovered() {
                        let b = Rect::from_min_size(rect.right_top() + vec2(-38.0, 8.0), vec2(30.0, 30.0));
                        ui.painter().rect_filled(b, 6.0, Color32::from_black_alpha(150));
                        paint_icon(ui.painter(), b.center(), Align2::CENTER_CENTER, ic::FULL, 14.0, Color32::WHITE);
                    }
                    if resp.clicked() {
                        self.viewer = true;
                    }
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    let _ = resp;
                    // 액션
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_btn(ui, p, ic::FULL, "크게 보기 (Space)", false).clicked() { self.viewer = true; }
                        if icon_btn(ui, p, ic::OPEN, "기본 앱으로 열기", false).clicked() { let _ = open::that(&path); }
                        if icon_btn(ui, p, ic::FOLDER_OPEN, "탐색기에서 보기", false).clicked() { reveal(&path); }
                        if icon_btn(ui, p, ic::COPY, "경로 복사", false).clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                            self.toast(ui.ctx(), "경로를 복사했습니다");
                        }
                        if icon_btn(ui, p, ic::TAG, "해시(BLAKE3) 계산", false).clicked() {
                            self.hash = organizer::hash_file(&path).ok().map(|h| h.iter().map(|b| format!("{b:02x}")).collect());
                        }
                        if icon_btn(ui, p, ic::EDIT, "이름 변경 (F2)", self.renaming).clicked() { self.renaming = !self.renaming; }
                        if icon_btn(ui, p, ic::DELETE, "삭제 → 앱 휴지통 (Delete)", false).clicked() { self.confirm_delete = true; }
                    });
                    if self.renaming && self.sel.len() <= 1 {
                        ui.horizontal(|ui| {
                            let te = ui.add(TextEdit::singleline(&mut self.rename_buf).desired_width(ui.available_width() - 76.0));
                            let go = ui.add(Button::new("적용").fill(p.accent)).clicked() || (te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                            if go && !self.rename_buf.is_empty() {
                                let to = path.with_file_name(&self.rename_buf);
                                if to.exists() {
                                    self.toast(ui.ctx(), "같은 이름의 파일이 이미 있습니다");
                                } else if std::fs::rename(&path, &to).is_ok() {
                                    self.entries[pi].name = self.rename_buf.clone();
                                    self.entries[pi].path = to.clone();
                                    self.thumbs.remove(&path);
                                    self.renaming = false;
                                    self.set_primary(pi);
                                    self.toast(ui.ctx(), "이름을 변경했습니다");
                                } else {
                                    self.toast(ui.ctx(), "이름 변경에 실패했습니다");
                                }
                            }
                        });
                    }
                    if self.sel.len() <= 1 {
                        section(ui, p, ic::INFO, "상세 정보");
                        info_rows(ui, p, &self.info.rows);
                        if let Some(h) = &self.hash {
                            ui.label(RichText::new("BLAKE3").size(12.0).color(p.weak));
                            ui.add(Label::new(RichText::new(h).monospace().size(11.0)).wrap());
                        }
                        if !self.info.exif.is_empty() {
                            section(ui, p, ic::CAMERA, "EXIF");
                            info_rows(ui, p, &self.info.exif);
                        }
                    }
                });
            });
    }

    // ───────── 전역 오버레이 / 단축키 ─────────
    pub fn overlays(&mut self, ctx: &Context, p: &Palette) {
        self.shortcuts(ctx);
        if self.viewer {
            self.viewer_ui(ctx, p);
        }
        self.report_modal(ctx, p);
        self.quick_modal(ctx, p);
        // 삭제 확인
        if self.confirm_delete {
            let n = self.sel.len().max(1);
            let mut close = false;
            let r = Modal::new(Id::new("del")).frame(Frame::new().fill(p.bg).corner_radius(12.0).stroke(Stroke::new(1.0, p.border)).inner_margin(22.0)).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.horizontal(|ui| {
                    ui.label(icon(ic::WARN, 22.0, p.warn));
                    ui.label(RichText::new("앱 휴지통으로 이동").strong().size(17.0));
                });
                ui.add_space(6.0);
                ui.label(RichText::new(format!("선택한 이미지 {n}개를 휴지통으로 이동할까요?\n휴지통에서 복원할 수 있습니다.")).color(p.weak));
                ui.add_space(10.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(Button::new(RichText::new("  삭제  ").color(Color32::WHITE)).fill(p.danger)).clicked() {
                        self.delete_selected(ctx);
                        close = true;
                    }
                    if button(ui, p, "", "취소", false, true).clicked() {
                        close = true;
                    }
                });
            });
            if close || r.should_close() {
                self.confirm_delete = false;
            }
        }
        if self.about {
            let r = Modal::new(Id::new("about")).frame(Frame::new().fill(p.bg).corner_radius(12.0).stroke(Stroke::new(1.0, p.border)).inner_margin(24.0)).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    ui.painter().rect_filled(r, 12.0, p.accent);
                    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, ic::PHOTO, 24.0, p.on_accent);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Image Cleaner").strong().size(20.0));
                        ui.label(RichText::new(format!("버전 {} · 네이티브 Rust 애플리케이션", env!("CARGO_PKG_VERSION"))).color(p.weak));
                    });
                });
                ui.add_space(8.0);
                ui.label("완전히 동일한 이미지는 하나만 남기고, 이름 충돌은 자동으로 해결하며, 촬영 연도별 폴더로 정리해 주는 이미지 관리 프로그램입니다.");
                ui.add_space(8.0);
                self.update_section(ui, p);
                ui.add_space(8.0);
                ui.label(RichText::new("단축키").strong());
                egui::Grid::new("keys").num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
                    for (k, d) in [
                        ("Ctrl + F", "검색"), ("방향키 / Enter·Space", "이동 / 크게 보기"), ("Ctrl + A", "전체 선택"),
                        ("Ctrl·Shift + 클릭", "다중 선택"), ("F2", "이름 변경"), ("Delete", "삭제 (앱 휴지통으로)"),
                        ("F5", "새로고침"), ("Esc", "뷰어 닫기"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(p.accent));
                        ui.label(d);
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, p, "", "닫기", true, true).clicked() {
                        self.about = false;
                    }
                });
            });
            if r.should_close() {
                self.about = false;
            }
        }
        // 폴더 드래그 오버레이
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let area = Area::new(Id::new("drop")).order(Order::Foreground).fixed_pos(pos2(0.0, 0.0));
            area.show(ctx, |ui| {
                let r = ctx.content_rect();
                ui.painter().rect_filled(r, 0.0, p.accent.gamma_multiply(0.22));
                ui.painter().rect_stroke(r.shrink(14.0), 14.0, Stroke::new(3.0, p.accent), StrokeKind::Inside);
                ui.painter().text(r.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, ic::NEW_FOLDER, FontId::new(44.0, theme::icons()), p.accent);
                let msg = if self.tab == Tab::Organize { "여기에 놓아 원본 폴더로 지정" } else { "여기에 놓아 폴더 열기" };
                ui.painter().text(r.center() + vec2(0.0, 36.0), Align2::CENTER_CENTER, msg, FontId::proportional(20.0), p.accent);
                ui.allocate_rect(r, Sense::hover());
            });
        }
        // 토스트
        if let Some((msg, t0)) = self.toast.clone() {
            let now = ctx.input(|i| i.time);
            if now - t0 > 2.6 {
                self.toast = None;
            } else {
                Area::new(Id::new("toast")).order(Order::Tooltip).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -48.0)).show(ctx, |ui| {
                    Frame::new().fill(p.text).corner_radius(18.0).inner_margin(Margin::symmetric(18, 9)).show(ui, |ui| {
                        ui.label(RichText::new(msg).color(p.bg).size(13.5));
                    });
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    fn delete_selected(&mut self, ctx: &Context) {
        let mut idxs: Vec<usize> = if self.sel.is_empty() { self.primary.into_iter().collect() } else { self.sel.iter().copied().collect() };
        idxs.retain(|&i| i < self.entries.len());
        let paths: Vec<PathBuf> = idxs.iter().map(|&i| self.entries[i].path.clone()).collect();
        let n = paths.len();
        let (moved, errs) = crate::bin_store::move_in_many(&paths);
        match if errs.is_empty() { Ok(()) } else { Err(errs.join("; ")) } {
            Ok(_) => {
                let del: std::collections::HashSet<usize> = idxs.into_iter().collect();
                let old = std::mem::take(&mut self.entries);
                self.entries = old.into_iter().enumerate().filter(|(i, _)| !del.contains(i)).map(|(_, e)| e).collect();
                self.sel.clear();
                self.primary = None;
                self.preview = None;
                self.viewer = false;
                self.bin_dirty = true;
                self.toast(ctx, format!("{n}개를 앱 휴지통으로 이동했습니다"));
            }
            Err(e) => {
                self.bin_dirty = true;
                self.toast(ctx, format!("{moved}/{n}개 이동, 일부 실패: {e}"));
            }
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (f5, ctrl_f, ctrl_a, del, f2, esc, l, r, u, d, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::F5), i.modifiers.command && i.key_pressed(Key::F), i.modifiers.command && i.key_pressed(Key::A),
                i.key_pressed(Key::Delete), i.key_pressed(Key::F2), i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter) || i.key_pressed(Key::Space),
            )
        });
        if ctrl_f {
            self.focus_search = true;
        }
        if self.tab != Tab::Browse {
            return;
        }
        if f5 {
            self.kids.clear();
            self.reload();
        }
        if esc && self.viewer {
            self.viewer = false;
            return;
        }
        if ctrl_a {
            self.sel = self.visible().into_iter().collect();
        }
        if del && self.primary.is_some() {
            self.confirm_delete = true;
        }
        if f2 && self.primary.is_some() {
            self.renaming = true;
        }
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let cols = if self.viewer { 1 } else { self.cols as isize };
        let delta: isize = if l { -1 } else if r { 1 } else if u && !self.viewer { -cols } else if d && !self.viewer { cols } else { 0 };
        if delta != 0 {
            let cur = self.primary.and_then(|s| vis.iter().position(|&v| v == s));
            let t = match cur {
                Some(c) => (c as isize + delta).clamp(0, vis.len() as isize - 1) as usize,
                None => 0,
            };
            self.select_only(vis[t]);
            self.scroll_to = Some(t / self.cols.max(1));
        }
        if enter && self.primary.is_some() {
            self.viewer = true;
        }
    }

    // ───────── 전체화면 뷰어 ─────────
    fn viewer_ui(&mut self, ctx: &Context, p: &Palette) {
        let Some(pi) = self.primary.filter(|&i| i < self.entries.len()) else {
            self.viewer = false;
            return;
        };
        let vis = self.visible();
        let pos = vis.iter().position(|&v| v == pi).unwrap_or(0);
        let mut nav: isize = 0;
        let mut close = false;
        Area::new(Id::new("viewer")).order(Order::Foreground).fixed_pos(pos2(0.0, 0.0)).show(ctx, |ui| {
            let r = ctx.content_rect();
            ui.painter().rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(8, 9, 11, 244));
            let resp = ui.allocate_rect(r, Sense::click_and_drag());
            if resp.dragged() {
                self.voff += resp.drag_delta();
            }
            if resp.double_clicked() {
                self.vzoom = if self.vzoom > 1.05 { 1.0 } else { 2.5 };
                self.voff = Vec2::ZERO;
            }
            if resp.hovered() {
                let sc = ctx.input(|i| i.smooth_scroll_delta.y);
                if sc != 0.0 {
                    self.vzoom = (self.vzoom * (sc * 0.0016).exp()).clamp(0.2, 16.0);
                }
            }
            let area = Rect::from_min_max(r.min + vec2(70.0, 56.0), r.max - vec2(70.0, 20.0));
            match &self.preview {
                Some((_, Some(t))) => {
                    let s = t.size_vec2();
                    let k = (area.width() / s.x).min(area.height() / s.y) * self.vzoom;
                    let ir = Rect::from_center_size(area.center() + self.voff, s * k);
                    ui.painter().with_clip_rect(r).add(epaint::RectShape::filled(ir, 0.0, Color32::WHITE).with_texture(t.id(), UV));
                }
                _ => spinner(ui.painter(), r.center(), 16.0, Color32::WHITE, ctx.input(|i| i.time)),
            }
            // 상단 바
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 48.0)), 0.0, Color32::from_black_alpha(140));
            ui.painter().text(r.min + vec2(20.0, 24.0), Align2::LEFT_CENTER, &self.entries[pi].name, FontId::proportional(15.0), Color32::WHITE);
            let dim = self.info.dims.map(|(w, h)| format!("   {w}×{h}")).unwrap_or_default();
            ui.painter().text(r.center_top() + vec2(0.0, 24.0), Align2::CENTER_CENTER, format!("{} / {}{}   ·   {:.0}%", pos + 1, vis.len(), dim, self.vzoom * 100.0), FontId::proportional(13.0), Color32::from_gray(190));
            let vb = |ui: &mut Ui, rect: Rect, glyph: &str, id: &str| -> bool {
                let rs = ui.interact(rect, Id::new(id), Sense::click());
                if rs.hovered() {
                    ui.painter().rect_filled(rect, 8.0, Color32::from_white_alpha(30));
                }
                paint_icon(ui.painter(), rect.center(), Align2::CENTER_CENTER, glyph, 16.0, Color32::WHITE);
                rs.clicked()
            };
            if vb(ui, Rect::from_min_size(r.right_top() + vec2(-48.0, 8.0), vec2(34.0, 32.0)), ic::CLOSE, "vclose") {
                close = true;
            }
            if vb(ui, Rect::from_min_size(r.right_top() + vec2(-92.0, 8.0), vec2(34.0, 32.0)), ic::FULL, "vfit") {
                self.vzoom = 1.0;
                self.voff = Vec2::ZERO;
            }
            if pos > 0 && vb(ui, Rect::from_center_size(pos2(r.left() + 34.0, r.center().y), vec2(44.0, 64.0)), ic::LEFT, "vprev") {
                nav = -1;
            }
            if pos + 1 < vis.len() && vb(ui, Rect::from_center_size(pos2(r.right() - 34.0, r.center().y), vec2(44.0, 64.0)), ic::RIGHT, "vnext") {
                nav = 1;
            }
            ui.painter().text(r.center_bottom() - vec2(0.0, 10.0), Align2::CENTER_CENTER, "휠: 확대/축소 · 드래그: 이동 · 더블클릭: 확대 전환 · ←/→: 이전/다음 · Esc: 닫기", FontId::proportional(12.0), Color32::from_gray(130));
        });
        if close {
            self.viewer = false;
        }
        if nav != 0 {
            let t = (pos as isize + nav).clamp(0, vis.len() as isize - 1) as usize;
            self.select_only(vis[t]);
        }
    }
}

fn paint_chev(ui: &mut Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 28.0), Sense::hover());
    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, ic::CHEV_R, 9.0, p.weak);
}

fn paint_icon_inline(ui: &mut Ui, p: &Palette, glyph: &str, size: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(size + 2.0, 28.0), Sense::hover());
    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, glyph, size, p.weak);
}

fn section(ui: &mut Ui, p: &Palette, glyph: &str, title: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(icon(glyph, 14.0, p.accent));
        ui.label(RichText::new(title).strong().size(14.0));
    });
}

fn info_rows(ui: &mut Ui, p: &Palette, rows: &[(String, String)]) {
    let w = ui.available_width();
    Frame::new().fill(p.card).corner_radius(10.0).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(w - 26.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (k, v)) in rows.iter().enumerate() {
            if i > 0 {
                let y = ui.cursor().top();
                ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
            }
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(78.0, 18.0), Layout::left_to_right(Align::TOP), |ui| {
                    ui.label(RichText::new(k).size(12.5).color(p.weak));
                });
                ui.add(Label::new(RichText::new(v).size(13.0)).wrap());
            });
            ui.add_space(6.0);
        }
    });
}

pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
}
