// SPDX-License-Identifier: GPL-3.0-or-later
use crate::app::*;
use crate::bin_store;
use crate::theme::{button, card, fmt_num, fmt_size, ic, icon, paint_icon, Palette};
use eframe::egui::{self, epaint, *};

const UV: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };

fn fmt_ts(secs: u64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(secs as i64, 0).single().map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

impl App {
    pub fn refresh_bin(&mut self) {
        self.bin_items = bin_store::list();
        let ids: std::collections::HashSet<&String> = self.bin_items.iter().map(|i| &i.id).collect();
        self.bin_sel.retain(|s| ids.contains(s));
        self.bin_dirty = false;
    }

    pub fn bin_page(&mut self, ui: &mut Ui, p: &Palette) {
        if self.bin_dirty {
            self.refresh_bin();
        }
        let total: u64 = self.bin_items.iter().map(|i| i.size).sum();
        let n = self.bin_items.len();
        let ctx = ui.ctx().clone();
        let mut restore: Vec<String> = vec![];
        let mut purge: Vec<String> = vec![];
        Frame::new().inner_margin(Margin::same(26)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(RichText::new("휴지통").size(24.0).strong());
                    ui.label(RichText::new(format!("앱에서 삭제한 이미지를 보관합니다 · {}개 · {}", fmt_num(n), fmt_size(total))).color(p.weak));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, p, ic::DELETE, "휴지통 비우기", false, n > 0).clicked() {
                        self.bin_confirm_empty = true;
                    }
                    let has_sel = !self.bin_sel.is_empty();
                    if button(ui, p, ic::CLOSE, "선택 영구 삭제", false, has_sel).clicked() {
                        purge = self.bin_sel.iter().cloned().collect();
                    }
                    if button(ui, p, ic::BACK, "선택 복원", true, has_sel).clicked() {
                        restore = self.bin_sel.iter().cloned().collect();
                    }
                });
            });
            if n == 0 {
                let r = ui.max_rect();
                let c = pos2(r.center().x, r.top() + 200.0);
                ui.painter().circle_filled(c, 46.0, p.card);
                paint_icon(ui.painter(), c, Align2::CENTER_CENTER, ic::DELETE, 34.0, p.weak);
                ui.painter().text(c + vec2(0.0, 76.0), Align2::CENTER_CENTER, "휴지통이 비어 있습니다", FontId::proportional(18.0), p.text);
                ui.painter().text(c + vec2(0.0, 104.0), Align2::CENTER_CENTER, "삭제한 이미지와 '원본 삭제'로 정리된 원본이 여기에 보관돼요", FontId::proportional(13.0), p.weak);
                return;
            }
            // 전체 선택
            ui.horizontal(|ui| {
                let all = self.bin_sel.len() == n;
                if ui.add(Button::new(if all { "전체 해제" } else { "전체 선택" })).clicked() {
                    if all { self.bin_sel.clear() } else { self.bin_sel = self.bin_items.iter().map(|i| i.id.clone()).collect() }
                }
                if !self.bin_sel.is_empty() {
                    ui.label(RichText::new(format!("{}개 선택됨", self.bin_sel.len())).color(p.weak));
                }
            });
            let items = self.bin_items.clone();
            card(ui, p, |ui| {
                ui.set_min_width(ui.available_width());
                ScrollArea::vertical().auto_shrink(false).max_height(ui.available_height() - 20.0).show_rows(ui, 64.0, items.len(), |ui, range| {
                    for i in range {
                        let it = &items[i];
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                        let sel = self.bin_sel.contains(&it.id);
                        if sel {
                            ui.painter().rect_filled(rect, 8.0, p.accent_soft);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 8.0, p.hover);
                        }
                        // 체크 원
                        let cb = pos2(rect.left() + 20.0, rect.center().y);
                        if sel {
                            ui.painter().circle_filled(cb, 10.0, p.accent);
                            paint_icon(ui.painter(), cb, Align2::CENTER_CENTER, ic::CHECK, 11.0, p.on_accent);
                        } else {
                            ui.painter().circle_stroke(cb, 9.0, Stroke::new(1.5, p.weak));
                        }
                        // 썸네일
                        let tr = Rect::from_min_size(pos2(rect.left() + 42.0, rect.top() + 5.0), vec2(48.0, 48.0));
                        ui.painter().rect_filled(tr, 6.0, p.widget);
                        match self.thumbs.get(&it.stored) {
                            Some(Some(t)) => {
                                let s = t.size_vec2();
                                let k = (tr.width() / s.x).min(tr.height() / s.y);
                                let r = Rect::from_center_size(tr.center(), s * k);
                                ui.painter().add(epaint::RectShape::filled(r, 4.0, Color32::WHITE).with_texture(t.id(), UV));
                            }
                            Some(None) => {}
                            None => {
                                if self.pending.len() < 48 && self.pending.insert(it.stored.clone()) {
                                    let _ = self.loader.req_tx.send(Req { path: it.stored.clone(), kind: Kind::Thumb });
                                }
                            }
                        }
                        let name = it.original.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let dir = it.original.parent().map(|d| d.display().to_string()).unwrap_or_default();
                        let tx = rect.left() + 104.0;
                        let tw = rect.right() - tx - 120.0;
                        let clip = Rect::from_min_size(pos2(tx, rect.top()), vec2(tw.max(40.0), rect.height()));
                        ui.painter().with_clip_rect(clip).text(pos2(tx, rect.top() + 19.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), p.text);
                        ui.painter().with_clip_rect(clip).text(pos2(tx, rect.top() + 39.0), Align2::LEFT_CENTER, format!("원래 위치: {dir}"), FontId::proportional(12.0), p.weak);
                        let rx = rect.right() - 12.0;
                        ui.painter().text(pos2(rx, rect.top() + 19.0), Align2::RIGHT_CENTER, fmt_ts(it.deleted_at), FontId::proportional(12.5), p.weak);
                        ui.painter().text(pos2(rx, rect.top() + 39.0), Align2::RIGHT_CENTER, fmt_size(it.size), FontId::proportional(12.5), p.weak);
                        if resp.clicked() && !self.bin_sel.remove(&it.id) {
                            self.bin_sel.insert(it.id.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button("복원").clicked() { restore = vec![it.id.clone()]; ui.close(); }
                            if ui.button("영구 삭제").clicked() { purge = vec![it.id.clone()]; ui.close(); }
                        });
                    }
                });
            });
        });
        if !restore.is_empty() {
            let (mut ok, mut last) = (0, None);
            for id in &restore {
                if let Ok(path) = bin_store::restore(id) {
                    ok += 1;
                    last = Some(path);
                }
            }
            self.bin_sel.clear();
            self.bin_dirty = true;
            self.reload();
            let msg = match (ok, last) {
                (1, Some(p)) => format!("복원했습니다: {}", p.display()),
                _ => format!("{ok}개를 원래 위치로 복원했습니다"),
            };
            self.toast(&ctx, msg);
        }
        if !purge.is_empty() {
            for id in &purge {
                let _ = bin_store::purge(id);
            }
            self.bin_sel.clear();
            self.bin_dirty = true;
            self.toast(&ctx, format!("{}개를 영구 삭제했습니다", purge.len()));
        }
        if self.bin_confirm_empty {
            let mut close = false;
            let r = Modal::new(Id::new("empty_bin")).frame(Frame::new().fill(p.bg).corner_radius(12.0).stroke(Stroke::new(1.0, p.border)).inner_margin(22.0)).show(&ctx, |ui| {
                ui.set_width(400.0);
                ui.horizontal(|ui| {
                    ui.label(icon(ic::WARN, 22.0, p.danger));
                    ui.label(RichText::new("휴지통 비우기").strong().size(17.0));
                });
                ui.label(RichText::new(format!("{}개 파일({})이 영구 삭제되며 복원할 수 없습니다.", fmt_num(n), fmt_size(total))).color(p.weak));
                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(Button::new(RichText::new("  모두 삭제  ").color(Color32::WHITE)).fill(p.danger)).clicked() {
                        let k = bin_store::empty().unwrap_or(0);
                        self.bin_dirty = true;
                        self.bin_sel.clear();
                        self.toast(&ctx, format!("휴지통을 비웠습니다 ({k}개)"));
                        close = true;
                    }
                    if button(ui, p, "", "취소", false, true).clicked() {
                        close = true;
                    }
                });
            });
            if close || r.should_close() {
                self.bin_confirm_empty = false;
            }
        }
    }
}
