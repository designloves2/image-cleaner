// SPDX-License-Identifier: GPL-3.0-or-later
//! 테마(라이트/다크), 폰트, 아이콘 코드, 공용 커스텀 위젯. 모두 flat(그림자 없음).

use eframe::egui::{self, epaint, *};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,       // 메인 작업 영역
    pub sidebar: Color32,  // 좌/우 패널
    pub bar: Color32,      // 상단 바 / 상태바
    pub card: Color32,     // 카드
    pub widget: Color32,   // 입력창, 버튼 기본
    pub hover: Color32,
    pub active: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub danger: Color32,
}

pub fn palette(dark: bool) -> Palette {
    let c = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        Palette {
            bg: c(0x141518), sidebar: c(0x1A1B1F), bar: c(0x1A1B1F), card: c(0x202228),
            widget: c(0x2A2D35), hover: c(0x343843), active: c(0x3E4350), border: c(0x2C2F37),
            text: c(0xE6E8EC), weak: c(0x8E94A0), accent: c(0x3B82F6), accent_soft: c(0x1F3358),
            on_accent: c(0xFFFFFF), success: c(0x22C55E), warn: c(0xF59E0B), danger: c(0xEF4444),
        }
    } else {
        Palette {
            bg: c(0xFFFFFF), sidebar: c(0xF5F6F8), bar: c(0xF5F6F8), card: c(0xF5F6F8),
            widget: c(0xE9ECF0), hover: c(0xDEE2E8), active: c(0xCFD5DD), border: c(0xE1E4E9),
            text: c(0x1B1E24), weak: c(0x6B7280), accent: c(0x2563EB), accent_soft: c(0xDFE9FD),
            on_accent: c(0xFFFFFF), success: c(0x16A34A), warn: c(0xD97706), danger: c(0xDC2626),
        }
    }
}

pub fn apply_theme(ctx: &Context, dark: bool) {
    let p = palette(dark);
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.extreme_bg_color = p.widget;
    v.faint_bg_color = p.card;
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.accent;
    v.window_shadow = epaint::Shadow::NONE;
    v.popup_shadow = epaint::Shadow::NONE;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, p.on_accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    let r = CornerRadius::same(6);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.bg),
        (&mut v.widgets.inactive, p.widget),
        (&mut v.widgets.hovered, p.hover),
        (&mut v.widgets.active, p.active),
        (&mut v.widgets.open, p.widget),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = r;
        w.expansion = 0.0;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, p.text);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    ctx.set_visuals_of(if dark { Theme::Dark } else { Theme::Light }, v);
    ctx.set_theme(if dark { ThemePreference::Dark } else { ThemePreference::Light });
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.scroll = style::ScrollStyle::floating();
        s.interaction.selectable_labels = false;
        s.text_styles = [
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
    });
}

pub fn icons() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn setup_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, file: &str, first: bool, mono: bool| -> bool {
        let Ok(d) = std::fs::read(file) else { return false };
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(d)));
        let mut fams = vec![FontFamily::Proportional];
        if mono {
            fams.push(FontFamily::Monospace);
        }
        for f in fams {
            let list = fonts.families.get_mut(&f).unwrap();
            if first { list.insert(0, name.into()) } else { list.push(name.into()) }
        }
        true
    };
    add(&mut fonts, "segoeui", "C:/Windows/Fonts/segoeui.ttf", true, false);
    add(&mut fonts, "malgun", "C:/Windows/Fonts/malgun.ttf", false, true);
    add(&mut fonts, "consola", "C:/Windows/Fonts/consola.ttf", true, false);
    // consola 는 Monospace 우선
    if fonts.font_data.contains_key("consola") {
        if let Some(l) = fonts.families.get_mut(&FontFamily::Proportional) {
            l.retain(|n| n != "consola");
        }
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "consola".into());
    }
    // 아이콘 폰트 (Segoe MDL2 Assets / Segoe Fluent Icons)
    let mut icon_list: Vec<String> = vec![];
    for (name, file) in [("icons", "C:/Windows/Fonts/segmdl2.ttf"), ("icons2", "C:/Windows/Fonts/SegoeIcons.ttf")] {
        if let Ok(d) = std::fs::read(file) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(d)));
            icon_list.push(name.into());
        }
    }
    if icon_list.is_empty() {
        icon_list.push("malgun".into());
    }
    fonts.families.insert(icons(), icon_list);
    ctx.set_fonts(fonts);
}

// ── 아이콘 코드 (Segoe MDL2 Assets) ──
pub mod ic {
    pub const FOLDER: &str = "\u{E838}";
    pub const FOLDER_OPEN: &str = "\u{E838}";
    pub const CHEV_R: &str = "\u{E76C}";
    pub const CHEV_D: &str = "\u{E70D}";
    pub const SEARCH: &str = "\u{E721}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const SUN: &str = "\u{E706}";
    pub const MOON: &str = "\u{E708}";
    pub const DELETE: &str = "\u{E74D}";
    pub const COPY: &str = "\u{E8C8}";
    pub const OPEN: &str = "\u{E8A7}";
    pub const FULL: &str = "\u{E740}";
    pub const ZOOM_IN: &str = "\u{E8A3}";
    pub const ZOOM_OUT: &str = "\u{E71F}";
    pub const INFO: &str = "\u{E946}";
    pub const EDIT: &str = "\u{E70F}";
    pub const CLOSE: &str = "\u{E711}";
    pub const CHECK: &str = "\u{E73E}";
    pub const PHOTO: &str = "\u{E91B}";
    pub const HOME: &str = "\u{E80F}";
    pub const DOWNLOAD: &str = "\u{E896}";
    pub const DOC: &str = "\u{E8A5}";
    pub const DESKTOP: &str = "\u{E7F4}";
    pub const DRIVE: &str = "\u{EDA2}";
    pub const UP: &str = "\u{E74A}";
    pub const BACK: &str = "\u{E72B}";
    pub const NEW_FOLDER: &str = "\u{E8F4}";
    pub const TAG: &str = "\u{E8EC}";
    pub const SORT: &str = "\u{E8CB}";
    pub const SAVE: &str = "\u{E74E}";
    pub const PLAY: &str = "\u{E768}";
    pub const STOP: &str = "\u{E71A}";
    pub const CHART: &str = "\u{E9D2}";
    pub const CALENDAR: &str = "\u{E787}";
    pub const WARN: &str = "\u{E7BA}";
    pub const LEFT: &str = "\u{E76B}";
    pub const RIGHT: &str = "\u{E76C}";
    pub const CAMERA: &str = "\u{E722}";
    pub const SYNC: &str = "\u{E895}";
    pub const ARROW_R: &str = "\u{E72A}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const ANALYZE: &str = "\u{E9F9}";
    pub const ERROR: &str = "\u{EA39}";
    pub const SELECT_ALL: &str = "\u{E8B3}";
    pub const MERGE: &str = "\u{E71C}";
    pub const GRID: &str = "\u{F0E2}";
    pub const DUP: &str = "\u{F413}";
}

pub fn icon(s: &str, size: f32, color: Color32) -> RichText {
    RichText::new(s).family(icons()).size(size).color(color)
}

pub fn paint_icon(painter: &Painter, pos: Pos2, align: Align2, s: &str, size: f32, color: Color32) {
    painter.text(pos, align, s, FontId::new(size, icons()), color);
}

// ── 위젯 ──

/// 정사각 아이콘 버튼 (flat)
pub fn icon_btn(ui: &mut Ui, p: &Palette, s: &str, tip: &str, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if active { p.accent_soft } else if resp.is_pointer_button_down_on() { p.active } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, 6.0, fill);
        let col = if active { p.accent } else { p.text };
        paint_icon(ui.painter(), rect.center(), Align2::CENTER_CENTER, s, 16.0, col);
    }
    resp.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand)
}

/// 아이콘 + 라벨 버튼. primary=true 면 accent 채움.
pub fn button(ui: &mut Ui, p: &Palette, ico: &str, label: &str, primary: bool, enabled: bool) -> Response {
    let font = FontId::proportional(14.0);
    let tcol = if primary { p.on_accent } else { p.text };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), tcol);
    let iw = if ico.is_empty() { 0.0 } else { 22.0 };
    let w = galley.size().x + iw + 28.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(84.0), 34.0), if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if !enabled {
            (p.widget, Stroke::NONE)
        } else if primary {
            let f = if resp.is_pointer_button_down_on() { p.accent.gamma_multiply(0.8) } else if resp.hovered() { p.accent.gamma_multiply(0.9) } else { p.accent };
            (f, Stroke::NONE)
        } else {
            let f = if resp.is_pointer_button_down_on() { p.active } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
            (f, Stroke::new(1.0, p.border))
        };
        ui.painter().rect(rect, 6.0, fill, stroke, StrokeKind::Inside);
        let col = if enabled { tcol } else { p.weak };
        let total = galley.size().x + iw;
        let x0 = rect.center().x - total / 2.0;
        if !ico.is_empty() {
            paint_icon(ui.painter(), pos2(x0 + 8.0, rect.center().y), Align2::CENTER_CENTER, ico, 15.0, col);
        }
        ui.painter().galley(pos2(x0 + iw, rect.center().y - galley.size().y / 2.0), galley, col);
    }
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

/// 토글 스위치
pub fn switch(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let track = p.widget.lerp_to_gamma(p.accent, t);
    ui.painter().rect_filled(rect, 11.0, track);
    let x = rect.left() + 11.0 + t * 18.0;
    ui.painter().circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// 세그먼트 컨트롤. items: (값, 아이콘, 라벨)
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, p: &Palette, value: &mut T, items: &[(T, &str, &str)]) -> bool {
    let font = FontId::proportional(14.0);
    let widths: Vec<f32> = items
        .iter()
        .map(|(_, i, l)| {
            let w = ui.painter().layout_no_wrap(l.to_string(), font.clone(), p.text).size().x;
            w + if i.is_empty() { 0.0 } else { 22.0 } + 28.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 34.0), Sense::hover());
    ui.painter().rect_filled(rect, 8.0, p.widget);
    let mut x = rect.left() + 3.0;
    let mut changed = false;
    for (n, (v, i, l)) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(widths[n], 28.0));
        let resp = ui.interact(r, ui.id().with(("seg", n)), Sense::click());
        let sel = *value == *v;
        if sel {
            ui.painter().rect_filled(r, 6.0, p.bg);
            ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 6.0, p.hover);
        }
        let col = if sel { p.accent } else { p.text };
        let g = ui.painter().layout_no_wrap(l.to_string(), font.clone(), col);
        let iw = if i.is_empty() { 0.0 } else { 22.0 };
        let x0 = r.center().x - (g.size().x + iw) / 2.0;
        if !i.is_empty() {
            paint_icon(ui.painter(), pos2(x0 + 8.0, r.center().y), Align2::CENTER_CENTER, i, 15.0, col);
        }
        ui.painter().galley(pos2(x0 + iw, r.center().y - g.size().y / 2.0), g, col);
        if resp.clicked() && !sel {
            *value = *v;
            changed = true;
        }
        x += widths[n];
    }
    changed
}

pub fn card<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    Frame::new()
        .fill(p.card)
        .corner_radius(10.0)
        .stroke(Stroke::new(1.0, p.border))
        .inner_margin(16.0)
        .show(ui, add)
}

/// 통계 타일: 큰 숫자 + 라벨
pub fn stat_tile(ui: &mut Ui, p: &Palette, ico: &str, value: &str, label: &str, color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 84.0), Sense::hover());
    ui.painter().rect(rect, 10.0, p.card, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let badge = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(28.0, 28.0));
    ui.painter().rect_filled(badge, 7.0, color.gamma_multiply(0.18));
    paint_icon(ui.painter(), badge.center(), Align2::CENTER_CENTER, ico, 14.0, color);
    ui.painter().text(rect.min + vec2(52.0, 28.0), Align2::LEFT_CENTER, label, FontId::proportional(12.5), p.weak);
    ui.painter().text(rect.min + vec2(14.0, 64.0), Align2::LEFT_CENTER, value, FontId::proportional(22.0), p.text);
}

pub fn arc(painter: &Painter, c: Pos2, r: f32, start: f32, sweep: f32, stroke: Stroke) {
    let n = ((sweep.abs() / 0.08) as usize).max(6);
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = start + sweep * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    painter.add(Shape::line(pts, stroke));
}

pub fn fmt_size(b: u64) -> String {
    let u = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", u[i]) }
}

pub fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
