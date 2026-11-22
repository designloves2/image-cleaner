// SPDX-License-Identifier: GPL-3.0-or-later
use crate::app::*;
use crate::theme::{button, ic, icon, Palette};
use crate::updater::{self, UpdateState};
use eframe::egui::*;

impl App {
    fn upd(&self) -> UpdateState {
        self.update.lock().unwrap().clone()
    }

    /// 새 버전이 있을 때만 나타나는 상단 배너
    pub fn update_banner(&mut self, ui: &mut Ui, p: &Palette) {
        let st = self.upd();
        if !matches!(st, UpdateState::Available(_) | UpdateState::Downloading(..) | UpdateState::Ready(..)) {
            return;
        }
        let ctx = ui.ctx().clone();
        Panel::top("updatebar")
            .exact_size(42.0)
            .frame(Frame::new().fill(p.accent_soft).inner_margin(Margin::symmetric(16, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(icon(ic::SYNC, 16.0, p.accent));
                    match &st {
                        UpdateState::Available(r) => {
                            ui.label(RichText::new(format!("새 버전 v{} 을(를) 사용할 수 있습니다", r.version)).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button(ui, p, "", "나중에", false, true).clicked() {
                                    *self.update.lock().unwrap() = UpdateState::Idle;
                                }
                                if button(ui, p, ic::DOWNLOAD, "지금 업데이트", true, true).clicked() {
                                    updater::download(self.update.clone(), ctx.clone(), r.clone());
                                }
                            });
                        }
                        UpdateState::Downloading(r, f) => {
                            ui.label(RichText::new(format!("v{} 다운로드 중… {:.0}%", r.version, f * 100.0)).strong());
                            let (bar, _) = ui.allocate_exact_size(vec2(240.0, 8.0), Sense::hover());
                            ui.painter().rect_filled(bar, 4.0, p.widget);
                            ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, 8.0)), 4.0, p.accent);
                        }
                        UpdateState::Ready(r, path) => {
                            ui.label(RichText::new(format!("v{} 업데이트 준비 완료 — 재시작하면 설치됩니다", r.version)).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button(ui, p, "", "나중에", false, true).clicked() {
                                    *self.update.lock().unwrap() = UpdateState::Idle;
                                }
                                if button(ui, p, ic::SYNC, "지금 재시작하여 설치", true, true).clicked() {
                                    updater::install_and_exit(path);
                                }
                            });
                        }
                        _ => {}
                    }
                });
            });
    }

    /// 정보 대화상자 안의 업데이트 / 라이선스 영역
    pub fn update_section(&mut self, ui: &mut Ui, p: &Palette) {
        let st = self.upd();
        ui.label(RichText::new("업데이트").strong());
        ui.horizontal(|ui| {
            let status = match &st {
                UpdateState::Idle => format!("현재 버전 v{}", updater::CURRENT),
                UpdateState::Checking => "확인 중…".into(),
                UpdateState::UpToDate => format!("최신 버전입니다 (v{})", updater::CURRENT),
                UpdateState::Available(r) => format!("새 버전 v{} 사용 가능", r.version),
                UpdateState::Downloading(_, f) => format!("다운로드 중 {:.0}%", f * 100.0),
                UpdateState::Ready(r, _) => format!("v{} 설치 준비 완료", r.version),
                UpdateState::Failed(e) => format!("확인 실패: {e}"),
            };
            ui.label(RichText::new(status).color(p.weak));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, p, ic::SYNC, "업데이트 확인", false, !matches!(st, UpdateState::Checking | UpdateState::Downloading(..))).clicked() {
                    updater::check(self.update.clone(), ui.ctx().clone(), false);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("시작할 때 자동으로 확인하고 내려받기");
                ui.label(RichText::new("GitHub 릴리스만 조회하며 개인 정보는 전송하지 않습니다.").size(12.0).color(p.weak));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if crate::theme::switch(ui, p, &mut self.auto_update).changed() {
                    self.save_settings();
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("GPL-3.0-or-later 라이선스 오픈소스").size(12.5).color(p.weak));
            ui.hyperlink_to(RichText::new("GitHub").size(12.5), format!("https://github.com/{}", updater::REPO));
        });
    }
}
