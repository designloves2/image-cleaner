// SPDX-License-Identifier: GPL-3.0-or-later
use crate::app::*;
use crate::theme::{self, arc, button, card, fmt_num, fmt_size, ic, icon, paint_icon, segmented, stat_tile, switch, Palette};
use eframe::egui::{self, epaint, *};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// 옵션 카드: 연도별 분류 + 원본 처리(유지 / 앱 휴지통으로 보관)
pub fn options_card(ui: &mut Ui, p: &Palette, by_year: &mut bool, suffix_all: &mut bool, trash_source: &mut bool, enabled: bool) {
    card(ui, p, |ui| {
        ui.set_min_width(ui.available_width());
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("연도별 폴더로 분류").strong());
                    ui.label(RichText::new("EXIF 촬영일(없으면 파일 생성일) 기준으로 2023, 2024 … 폴더 생성").size(12.5).color(p.weak));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    switch(ui, p, by_year);
                });
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("모든 파일명에 원본 폴더명 붙이기").strong());
                    ui.label(RichText::new("여러 하위 폴더를 합칠 때 IMG_001_여행.jpg 처럼 출처 폴더명을 접미사로 추가").size(12.5).color(p.weak));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    switch(ui, p, suffix_all);
                });
            });
            if !*suffix_all {
                ui.label(RichText::new("꺼져 있어도, 이미지는 다른데 파일명만 같은 경우에는 두 파일 모두 폴더명 접미사로 구분합니다 (번호 괄호 사용 안 함).").size(12.0).color(p.weak));
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(RichText::new("정리가 끝난 뒤 원본").strong());
            let mut mode = *trash_source;
            segmented(ui, p, &mut mode, &[(false, ic::FOLDER, "원본 유지"), (true, ic::DELETE, "원본 삭제 (앱 휴지통에 보관)")]);
            *trash_source = mode;
            let (txt, col) = if *trash_source {
                ("정리가 완료된 원본은 앱 내 휴지통으로 이동하며, '휴지통' 탭에서 언제든 복원하거나 비울 수 있어요.", p.warn)
            } else {
                ("원본 파일은 그대로 두고 출력 폴더로 복사합니다. (가장 안전)", p.weak)
            };
            ui.label(RichText::new(txt).size(12.5).color(col));
        });
    });
}

fn folder_field(ui: &mut Ui, p: &Palette, ico: &str, title: &str, hint: &str, val: &mut String, enabled: bool, must_exist: bool) {
    card(ui, p, |ui| {
        ui.set_min_width(ui.available_width());
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                ui.painter().rect_filled(r, 9.0, p.accent_soft);
                paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, ico, 17.0, p.accent);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(title).strong());
                    ui.label(RichText::new(hint).size(12.5).color(p.weak));
                });
            });
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(val).desired_width(ui.available_width() - 104.0).hint_text("폴더 경로"));
                if button(ui, p, "", "선택…", false, true).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        *val = d.display().to_string();
                    }
                }
            });
            if must_exist && !val.is_empty() && !Path::new(val.as_str()).is_dir() {
                ui.label(RichText::new("폴더를 찾을 수 없습니다").size(12.5).color(p.danger));
            }
        });
    });
}

pub fn suggest_output(src: &Path) -> String {
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "이미지".into());
    src.parent().unwrap_or(src).join(format!("{name}_정리")).display().to_string()
}

fn eta_text(secs: f32) -> String {
    if !secs.is_finite() || secs > 359999.0 {
        return "--:--".into();
    }
    let s = secs as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl App {
    pub fn organize_page(&mut self, ui: &mut Ui, p: &Palette) {
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(26)).show(ui, |ui| {
                let w = ui.available_width();
                let left_w = if w > 1000.0 { 520.0 } else { (w * 0.48).max(380.0) };
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(vec2(left_w, 0.0), Layout::top_down(Align::Min), |ui| self.org_settings(ui, p));
                    ui.add_space(22.0);
                    let rw = (ui.available_width()).max(300.0);
                    ui.allocate_ui_with_layout(vec2(rw, 0.0), Layout::top_down(Align::Min), |ui| self.org_status(ui, p));
                });
            });
        });
    }

    fn org_settings(&mut self, ui: &mut Ui, p: &Palette) {
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.label(RichText::new("이미지 정리").size(24.0).strong());
        ui.label(RichText::new("하위 폴더까지 스캔해서 완전히 같은 이미지는 하나만 남기고, 한곳으로 병합합니다.").color(p.weak));
        let idle = self.org.state == RunState::Idle;
        folder_field(ui, p, ic::FOLDER_OPEN, "원본 폴더", "하위 폴더까지 모두 스캔합니다 (폴더를 끌어다 놓아도 돼요)", &mut self.org.src, idle, true);
        folder_field(ui, p, ic::NEW_FOLDER, "출력 폴더", "정리된 이미지가 모이는 곳", &mut self.org.out, idle, false);
        let (mut y, mut sx, mut t) = (self.org.by_year, self.org.suffix_all, self.org.trash_source);
        options_card(ui, p, &mut y, &mut sx, &mut t, idle);
        self.org.by_year = y;
        self.org.suffix_all = sx;
        self.org.trash_source = t;
        let src_ok = Path::new(&self.org.src).is_dir();
        let ok = src_ok && !self.org.out.trim().is_empty() && self.org.src.trim_end_matches('\\') != self.org.out.trim_end_matches('\\');
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if idle {
                if button(ui, p, ic::PLAY, "정리 시작", true, ok).clicked() {
                    self.start_organize(false);
                }
                if button(ui, p, ic::ANALYZE, "미리 분석", false, ok).on_hover_text("파일을 건드리지 않고 결과만 미리 계산합니다").clicked() {
                    self.start_organize(true);
                }
            } else if button(ui, p, ic::STOP, "작업 취소", false, true).clicked() {
                self.org.cancel.store(true, Ordering::Relaxed);
            }
        });
    }

    fn org_status(&mut self, ui: &mut Ui, p: &Palette) {
        ui.spacing_mut().item_spacing.y = 12.0;
        if self.org.state == RunState::Running {
            self.processing_view(ui, p);
        } else if self.org.report.is_some() {
            self.result_summary(ui, p);
        } else {
            self.idle_view(ui, p);
        }
    }

    fn idle_view(&mut self, ui: &mut Ui, p: &Palette) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let t = ui.input(|i| i.time) as f32;
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("이렇게 정리돼요").strong().size(16.0));
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 190.0), Sense::hover());
            let xs = [rect.left() + rect.width() / 6.0, rect.center().x, rect.right() - rect.width() / 6.0];
            let y = rect.top() + 62.0;
            let nodes = [(ic::FOLDER_OPEN, "원본 폴더", "하위 폴더 전체"), (ic::MERGE, "중복 제거", "완전 동일 이미지 1개만"), (ic::CALENDAR, "출력 폴더", "촬영 연도별 정리")];
            for k in 0..2 {
                let (a, b) = (xs[k] + 44.0, xs[k + 1] - 44.0);
                ui.painter().hline(a..=b, y, Stroke::new(2.0, p.border));
                let ph = (t * 0.5 + k as f32 * 0.33).fract();
                ui.painter().circle_filled(pos2(a + (b - a) * ph, y), 4.0, p.accent);
                ui.painter().circle_filled(pos2(a + (b - a) * ((ph + 0.5).fract()), y), 3.0, p.accent.gamma_multiply(0.5));
            }
            for (i, (g, title, sub)) in nodes.iter().enumerate() {
                let c = pos2(xs[i], y);
                ui.painter().circle_filled(c, 36.0, p.accent_soft);
                paint_icon(ui.painter(), c, Align2::CENTER_CENTER, g, 28.0, p.accent);
                ui.painter().text(c + vec2(0.0, 58.0), Align2::CENTER_CENTER, *title, FontId::proportional(14.5), p.text);
                ui.painter().text(c + vec2(0.0, 78.0), Align2::CENTER_CENTER, *sub, FontId::proportional(12.0), p.weak);
            }
        });
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            for (a, b) in [
                ("내용이 완전히 같은 이미지만 제거", "바이트 단위(BLAKE3 해시)로 비교해 비슷한 사진은 지우지 않아요."),
                ("이름이 같아도 내용이 다르면 자동 변경", "photo.jpg, photo (1).jpg 처럼 덮어쓰기 없이 보존합니다."),
                ("실행 전에 미리 분석", "'미리 분석'으로 파일을 건드리지 않고 결과를 먼저 확인하세요."),
            ] {
                ui.horizontal_top(|ui| {
                    ui.label(icon(ic::CHECK, 14.0, p.success));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(a).strong());
                        ui.label(RichText::new(b).size(12.5).color(p.weak));
                    });
                });
            }
        });
    }

    // ───────── 처리 중 화면 (애니메이션 + 그래프) ─────────
    fn processing_view(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time);
        let o = &self.org;
        let w = ui.available_width();
        let steps: Vec<&str> = if o.dry {
            vec!["폴더 스캔", "중복 검사", "결과 분석"]
        } else if o.trash_source {
            vec!["폴더 스캔", "중복 검사", "복사·병합", "원본 보관"]
        } else {
            vec!["폴더 스캔", "중복 검사", "복사·병합"]
        };
        let stage = o.stage.min(steps.len() - 1);
        card(ui, p, |ui| {
            ui.set_min_width(w - 34.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(if o.dry { "미리 분석 중" } else { "정리하는 중" }).strong().size(17.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("경과 {}", eta_text(o.started.elapsed().as_secs_f32()))).color(p.weak));
                });
            });
            // 단계 표시기
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::hover());
            let n = steps.len();
            let step_w = rect.width() / n as f32;
            for i in 0..n {
                let c = pos2(rect.left() + step_w * (i as f32 + 0.5), rect.top() + 16.0);
                if i + 1 < n {
                    let done = i < stage;
                    ui.painter().hline(c.x + 18.0..=c.x + step_w - 18.0, c.y, Stroke::new(2.0, if done { p.success } else { p.border }));
                }
                if i < stage {
                    ui.painter().circle_filled(c, 13.0, p.success);
                    paint_icon(ui.painter(), c, Align2::CENTER_CENTER, ic::CHECK, 13.0, Color32::WHITE);
                } else if i == stage {
                    let pulse = (t * 3.0).sin() as f32 * 0.5 + 0.5;
                    ui.painter().circle_filled(c, 17.0 + pulse * 3.0, p.accent.gamma_multiply(0.18));
                    ui.painter().circle_filled(c, 13.0, p.accent);
                    arc(ui.painter(), c, 9.0, (t * 4.0) as f32, 3.4, Stroke::new(2.0, Color32::WHITE));
                } else {
                    ui.painter().circle_filled(c, 13.0, p.widget);
                    ui.painter().text(c, Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(12.5), p.weak);
                }
                let col = if i <= stage { p.text } else { p.weak };
                ui.painter().text(c + vec2(0.0, 32.0), Align2::CENTER_CENTER, steps[i], FontId::proportional(13.0), col);
            }
        });
        // 링 + 실시간 카운터
        let (done, total) = self.org.progress;
        let determinate = total > 0 && stage >= 1;
        let frac_target = if determinate { done as f32 / total as f32 } else { 0.0 };
        let frac = ctx.animate_value_with_time(Id::new("ring"), frac_target, 0.25);
        card(ui, p, |ui| {
            ui.set_min_width(w - 34.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(190.0, 190.0), Sense::hover());
                let c = rect.center();
                let r = 78.0;
                ui.painter().circle_stroke(c, r, Stroke::new(12.0, p.widget));
                if determinate {
                    arc(ui.painter(), c, r, -std::f32::consts::FRAC_PI_2, std::f32::consts::TAU * frac.clamp(0.001, 1.0), Stroke::new(12.0, p.accent));
                    ui.painter().text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("{:.0}%", frac * 100.0), FontId::proportional(38.0), p.text);
                } else {
                    let a = (t * 3.2) as f32;
                    arc(ui.painter(), c, r, a, 1.8, Stroke::new(12.0, p.accent));
                    arc(ui.painter(), c, r, a + 3.14, 0.9, Stroke::new(12.0, p.accent.gamma_multiply(0.4)));
                    ui.painter().text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, "···", FontId::proportional(34.0), p.text);
                }
                ui.painter().text(c + vec2(0.0, 28.0), Align2::CENTER_CENTER, &self.org.phase, FontId::proportional(12.5), p.weak);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    let l = self.org.live;
                    let tw = ((ui.available_width() - 10.0) / 2.0).max(120.0);
                    let anim = |id: &str, v: usize| ctx.animate_value_with_time(Id::new(id), v as f32, 0.2) as usize;
                    ui.horizontal(|ui| {
                        stat_tile(ui, p, ic::PHOTO, &fmt_num(anim("a", l.scanned)), "스캔한 이미지", p.accent, tw);
                        stat_tile(ui, p, ic::DUP, &fmt_num(anim("b", l.duplicates)), "중복 발견", p.warn, tw);
                    });
                    ui.horizontal(|ui| {
                        stat_tile(ui, p, ic::TAG, &fmt_num(anim("c", l.renamed)), "이름 자동 변경", p.accent, tw);
                        stat_tile(ui, p, ic::CHECK, &fmt_num(anim("d", l.copied)), "정리 완료", p.success, tw);
                    });
                });
            });
            ui.add_space(4.0);
            // 진행 막대 (shimmer)
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
            ui.painter().rect_filled(bar, 5.0, p.widget);
            if determinate {
                let fr = Rect::from_min_size(bar.min, vec2((bar.width() * frac).max(10.0), bar.height()));
                ui.painter().rect_filled(fr, 5.0, p.accent);
                let sx = fr.left() + ((t * 0.7) % 1.0) as f32 * (fr.width() + 80.0) - 80.0;
                let sh = Rect::from_min_max(pos2(sx, fr.top()), pos2(sx + 80.0, fr.bottom())).intersect(fr);
                if sh.is_positive() {
                    ui.painter().rect_filled(sh, 5.0, Color32::from_white_alpha(46));
                }
            } else {
                let x = bar.left() + (((t * 0.9) % 1.0) as f32) * (bar.width() + 140.0) - 140.0;
                let seg = Rect::from_min_max(pos2(x, bar.top()), pos2(x + 140.0, bar.bottom())).intersect(bar);
                if seg.is_positive() {
                    ui.painter().rect_filled(seg, 5.0, p.accent);
                }
            }
            let speed = {
                let s = &self.org.samples;
                if s.is_empty() { 0.0 } else { s.iter().rev().take(8).sum::<f32>() / s.iter().rev().take(8).count() as f32 }
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(if determinate { format!("{} / {}", fmt_num(done), fmt_num(total)) } else { "준비 중".into() }).color(p.weak));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if determinate && speed > 0.0 {
                        ui.label(RichText::new(format!("{speed:.0}개/초  ·  남은 시간 {}", eta_text((total - done) as f32 / speed))).color(p.weak));
                    }
                });
            });
        });
        // 처리 속도 그래프
        card(ui, p, |ui| {
            ui.set_min_width(w - 34.0);
            ui.label(RichText::new("처리 속도 (개/초)").strong());
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::hover());
            ui.painter().rect_filled(rect, 6.0, p.bg);
            let s = &self.org.samples;
            if s.len() >= 2 {
                let max = s.iter().cloned().fold(1.0_f32, f32::max) * 1.15;
                let pt = |i: usize| pos2(rect.left() + 6.0 + (rect.width() - 12.0) * i as f32 / (s.len() - 1) as f32, rect.bottom() - 6.0 - (rect.height() - 12.0) * (s[i] / max));
                for i in 0..s.len() - 1 {
                    let (a, b) = (pt(i), pt(i + 1));
                    ui.painter().add(Shape::convex_polygon(vec![a, b, pos2(b.x, rect.bottom() - 6.0), pos2(a.x, rect.bottom() - 6.0)], p.accent_soft, Stroke::NONE));
                }
                ui.painter().add(Shape::line((0..s.len()).map(pt).collect(), Stroke::new(2.0, p.accent)));
            } else {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "측정 중…", FontId::proportional(12.5), p.weak);
            }
        });
        // 현재 파일
        card(ui, p, |ui| {
            ui.set_min_width(w - 34.0);
            ui.horizontal(|ui| {
                ui.label(icon(ic::PHOTO, 15.0, p.accent));
                let dots = ".".repeat(((t * 2.0) as usize % 3) + 1);
                let cur = if self.org.current.is_empty() { "파일 목록을 만드는 중".to_string() } else { self.org.current.clone() };
                ui.add(Label::new(RichText::new(format!("{cur}{dots}")).size(13.0)).truncate());
            });
        });
    }

    // ───────── 완료 후 요약 ─────────
    fn result_summary(&mut self, ui: &mut Ui, p: &Palette) {
        let rep = self.org.report.as_ref().unwrap();
        let (dry, cancelled) = (rep.dry_run, rep.cancelled);
        let l = self.org.live;
        let (title, col, g) = if cancelled {
            ("작업이 취소되었습니다", p.warn, ic::WARN)
        } else if dry {
            ("미리 분석이 끝났습니다", p.accent, ic::ANALYZE)
        } else {
            ("정리가 완료되었습니다", p.success, ic::CHECK)
        };
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 22.0, col.gamma_multiply(0.18));
                paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, g, 20.0, col);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).strong().size(18.0));
                    ui.label(RichText::new(format!("소요 시간 {:.1}초", self.org.finished_in)).color(p.weak));
                });
            });
            ui.add_space(6.0);
            let tw = ((ui.available_width() - 20.0) / 3.0).max(110.0);
            ui.horizontal(|ui| {
                stat_tile(ui, p, ic::PHOTO, &fmt_num(l.scanned), "스캔한 이미지", p.accent, tw);
                stat_tile(ui, p, ic::DUP, &fmt_num(l.duplicates), "중복 제외", p.warn, tw);
                stat_tile(ui, p, ic::CHECK, &fmt_num(l.copied), if dry { "정리 예정" } else { "정리 완료" }, p.success, tw);
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if button(ui, p, ic::CHART, "결과 리포트", true, true).clicked() {
                    self.org.show_report = true;
                }
                if !dry && button(ui, p, ic::FOLDER_OPEN, "출력 폴더 열기", false, true).clicked() {
                    let _ = open::that(&self.org.out);
                }
                if dry && !cancelled && button(ui, p, ic::PLAY, "이 설정으로 정리 실행", false, true).clicked() {
                    self.start_organize(false);
                }
            });
        });
    }

    // ───────── 결과 리포트 창 ─────────
    pub fn report_modal(&mut self, ctx: &Context, p: &Palette) {
        if !self.org.show_report || self.org.report.is_none() {
            return;
        }
        let rep = self.org.report.take().unwrap();
        let mut tab = self.org.report_tab;
        let (mut close, mut run_now, mut open_out, mut csv) = (false, false, false, false);
        let out = self.org.out.clone();
        let secs = self.org.finished_in;
        let by_year = self.org.by_year;
        let r = Modal::new(Id::new("report"))
            .frame(Frame::new().fill(p.bg).corner_radius(14.0).stroke(Stroke::new(1.0, p.border)).inner_margin(24.0))
            .show(ctx, |ui| {
                ui.set_width(740.0);
                ui.spacing_mut().item_spacing.y = 12.0;
                let (title, col, g) = if rep.dry_run {
                    ("미리 분석 결과", p.accent, ic::ANALYZE)
                } else {
                    ("정리 결과 리포트", p.success, ic::CHECK)
                };
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 26.0, col.gamma_multiply(0.18));
                    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, g, 24.0, col);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(RichText::new(title).strong().size(21.0));
                        let sub = if rep.dry_run { "파일은 변경되지 않았습니다. 아래 결과대로 정리가 진행됩니다.".to_string() } else { format!("{:.1}초 만에 완료 · {out}", secs) };
                        ui.label(RichText::new(sub).color(p.weak).size(13.0));
                    });
                });
                let tw = (ui.available_width() - 20.0) / 3.0;
                ui.horizontal(|ui| {
                    stat_tile(ui, p, ic::PHOTO, &fmt_num(rep.scanned), "스캔한 이미지", p.accent, tw);
                    stat_tile(ui, p, ic::DUP, &fmt_num(rep.duplicates), "중복 제외", p.warn, tw);
                    stat_tile(ui, p, ic::CHECK, &fmt_num(rep.copied), if rep.dry_run { "정리 예정" } else { "정리 완료" }, p.success, tw);
                });
                ui.horizontal(|ui| {
                    stat_tile(ui, p, ic::TAG, &fmt_num(rep.renamed), "이름 자동 변경", p.accent, tw);
                    stat_tile(ui, p, ic::SAVE, &fmt_size(rep.saved_bytes), "절약한 용량", p.success, tw);
                    if rep.trashed > 0 {
                        stat_tile(ui, p, ic::DELETE, &fmt_num(rep.trashed), "원본 → 앱 휴지통", p.warn, tw);
                    } else {
                        stat_tile(ui, p, ic::ERROR, &fmt_num(rep.errors), "오류", if rep.errors > 0 { p.danger } else { p.weak }, tw);
                    }
                });
                let nd = rep.dup_list.len();
                let nr = rep.rename_list.len();
                segmented(ui, p, &mut tab, &[
                    (ReportTab::Overview, ic::CHART, "요약"),
                    (ReportTab::Duplicates, ic::DUP, &format!("중복 목록 ({})", fmt_num(rep.duplicates))),
                    (ReportTab::Renamed, ic::TAG, &format!("이름 변경 ({})", fmt_num(nr))),
                ]);
                let _ = nd;
                match tab {
                    ReportTab::Overview => {
                        // 고유/중복 비율 막대
                        if rep.scanned > 0 {
                            let dup_f = rep.duplicates as f32 / rep.scanned as f32;
                            ui.label(RichText::new(format!("중복 비율 {:.1}%", dup_f * 100.0)).strong());
                            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
                            ui.painter().rect_filled(bar, 7.0, p.success.gamma_multiply(0.85));
                            if dup_f > 0.0 {
                                let w = (bar.width() * dup_f).max(6.0);
                                ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(w, 14.0)), 7.0, p.warn);
                            }
                            ui.horizontal(|ui| {
                                ui.label(icon(ic::DUP, 11.0, p.warn));
                                ui.label(RichText::new("중복").size(12.5).color(p.weak));
                                ui.add_space(10.0);
                                ui.label(icon(ic::PHOTO, 11.0, p.success));
                                ui.label(RichText::new("고유 이미지").size(12.5).color(p.weak));
                            });
                        }
                        if !rep.years.is_empty() {
                            ui.label(RichText::new(if by_year { "연도별 이미지 수" } else { "정리된 이미지" }).strong());
                            let max = rep.years.values().copied().max().unwrap_or(1).max(1) as f32;
                            ScrollArea::vertical().max_height(170.0).auto_shrink([false, true]).show(ui, |ui| {
                                for (y, n) in &rep.years {
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(vec2(64.0, 20.0), Layout::left_to_right(Align::Center), |ui| {
                                            ui.label(RichText::new(y).monospace());
                                        });
                                        let bw = ui.available_width() - 70.0;
                                        let (r, _) = ui.allocate_exact_size(vec2(bw, 18.0), Sense::hover());
                                        ui.painter().rect_filled(r, 4.0, p.widget);
                                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2((bw * *n as f32 / max).max(4.0), 18.0)), 4.0, p.accent);
                                        ui.label(RichText::new(fmt_num(*n)).monospace());
                                    });
                                }
                            });
                        }
                    }
                    ReportTab::Duplicates => {
                        if rep.dup_list.is_empty() {
                            ui.label(RichText::new("중복된 이미지가 없습니다").color(p.weak));
                        } else {
                            ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show_rows(ui, 46.0, rep.dup_list.len(), |ui, range| {
                                for i in range {
                                    let (d, o) = &rep.dup_list[i];
                                    ui.horizontal(|ui| {
                                        ui.label(icon(ic::DUP, 15.0, p.warn));
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 1.0;
                                            ui.add(Label::new(RichText::new(d.display().to_string()).size(12.5)).truncate());
                                            ui.add(Label::new(RichText::new(format!("= 원본 유지: {}", o.display())).size(12.0).color(p.weak)).truncate());
                                        });
                                    });
                                }
                            });
                            if rep.duplicates > rep.dup_list.len() {
                                ui.label(RichText::new(format!("… 외 {}개 (CSV 저장 시에도 상위 {}개만 포함)", fmt_num(rep.duplicates - rep.dup_list.len()), fmt_num(rep.dup_list.len()))).size(12.0).color(p.weak));
                            }
                        }
                    }
                    ReportTab::Renamed => {
                        if rep.rename_list.is_empty() {
                            ui.label(RichText::new("이름이 변경된 파일이 없습니다").color(p.weak));
                        } else {
                            ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show_rows(ui, 46.0, rep.rename_list.len(), |ui, range| {
                                for i in range {
                                    let (s, n) = &rep.rename_list[i];
                                    ui.horizontal(|ui| {
                                        ui.label(icon(ic::TAG, 15.0, p.accent));
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 1.0;
                                            ui.add(Label::new(RichText::new(s.display().to_string()).size(12.5).color(p.weak)).truncate());
                                            ui.add(Label::new(RichText::new(format!("→ {n}")).size(13.0).strong()).truncate());
                                        });
                                    });
                                }
                            });
                        }
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if button(ui, p, ic::SAVE, "CSV로 저장", false, true).clicked() {
                        csv = true;
                    }
                    if !rep.dry_run && button(ui, p, ic::FOLDER_OPEN, "출력 폴더 열기", false, true).clicked() {
                        open_out = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if rep.dry_run && !rep.cancelled {
                            if button(ui, p, ic::PLAY, "이 설정으로 정리 실행", true, true).clicked() {
                                run_now = true;
                            }
                            if button(ui, p, "", "닫기", false, true).clicked() {
                                close = true;
                            }
                        } else if button(ui, p, "", "닫기", true, true).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if csv {
            if let Some(path) = rfd::FileDialog::new().set_file_name("image-cleaner-report.csv").add_filter("CSV", &["csv"]).save_file() {
                let msg = match write_csv(&path, &rep) {
                    Ok(_) => "리포트를 CSV로 저장했습니다".to_string(),
                    Err(e) => format!("저장 실패: {e}"),
                };
                self.toast(ctx, msg);
            }
        }
        if open_out {
            let _ = open::that(&self.org.out);
        }
        self.org.report_tab = tab;
        self.org.report = Some(rep);
        if close || r.should_close() {
            self.org.show_report = false;
        }
        if run_now {
            self.org.show_report = false;
            self.start_organize(false);
        }
    }

    // ───────── 트리 우클릭 → "현재 폴더 정리" 팝업 ─────────
    pub fn quick_modal(&mut self, ctx: &Context, p: &Palette) {
        let Some(q) = self.quick.as_mut() else { return };
        let (mut go, mut dry, mut close) = (false, false, false);
        let r = Modal::new(Id::new("quick"))
            .frame(Frame::new().fill(p.bg).corner_radius(14.0).stroke(Stroke::new(1.0, p.border)).inner_margin(24.0))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                    ui.painter().rect_filled(r, 11.0, p.accent);
                    paint_icon(ui.painter(), r.center(), Align2::CENTER_CENTER, ic::MERGE, 21.0, p.on_accent);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new("이 폴더 정리하기").strong().size(19.0));
                        ui.label(RichText::new("결과를 저장할 폴더를 정하면 바로 시작합니다").color(p.weak).size(13.0));
                    });
                });
                card(ui, p, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("원본 폴더").size(12.5).color(p.weak));
                    ui.horizontal(|ui| {
                        ui.label(icon(ic::FOLDER_OPEN, 15.0, p.warn));
                        ui.add(Label::new(RichText::new(q.src.display().to_string()).strong()).truncate());
                    });
                });
                let mut out = q.out.clone();
                folder_field(ui, p, ic::NEW_FOLDER, "결과 폴더", "정리된 이미지가 저장될 곳 (없으면 새로 만듭니다)", &mut out, true, false);
                q.out = out;
                options_card(ui, p, &mut q.by_year, &mut q.suffix_all, &mut q.trash_source, true);
                let ok = !q.out.trim().is_empty() && Path::new(&q.out) != q.src.as_path();
                ui.horizontal(|ui| {
                    if button(ui, p, "", "취소", false, true).clicked() {
                        close = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, p, ic::PLAY, "정리 시작", true, ok).clicked() {
                            go = true;
                        }
                        if button(ui, p, ic::ANALYZE, "미리 분석", false, ok).clicked() {
                            go = true;
                            dry = true;
                        }
                    });
                });
            });
        if go {
            let q = self.quick.take().unwrap();
            self.org.src = q.src.display().to_string();
            self.org.out = q.out;
            self.org.by_year = q.by_year;
            self.org.trash_source = q.trash_source;
            self.org.suffix_all = q.suffix_all;
            self.tab = Tab::Organize;
            self.start_organize(dry);
        } else if close || r.should_close() {
            self.quick = None;
        }
    }
}

fn write_csv(path: &Path, rep: &crate::organizer::Report) -> std::io::Result<()> {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut s = String::from("\u{FEFF}구분,파일,비고\n");
    for (d, o) in &rep.dup_list {
        s += &format!("중복,{},{}\n", q(&d.display().to_string()), q(&format!("원본: {}", o.display())));
    }
    for (src, n) in &rep.rename_list {
        s += &format!("이름변경,{},{}\n", q(&src.display().to_string()), q(n));
    }
    for (y, n) in &rep.years {
        s += &format!("연도별,{},{}\n", q(y), n);
    }
    std::fs::write(path, s)
}

#[allow(dead_code)]
fn _unused(_: PathBuf, _: epaint::Color32) {}
