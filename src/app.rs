// SPDX-License-Identifier: GPL-3.0-or-later
use crate::bin_store::BinItem;
use crate::organizer::{self, Live, Msg, Options, Report};
use crate::updater::{self, UpdateState};
use crate::theme::{self, ic, palette};
use eframe::egui::{self, *};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

// ───────── 이미지 로더 (백그라운드 스레드) ─────────

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Thumb,
    Preview,
}
pub struct Req {
    pub path: PathBuf,
    pub kind: Kind,
}
struct Res {
    path: PathBuf,
    kind: Kind,
    img: Option<ColorImage>,
}

fn exif_orientation(p: &Path) -> u32 {
    let Ok(f) = std::fs::File::open(p) else { return 1 };
    let mut br = std::io::BufReader::new(f);
    exif::Reader::new()
        .read_from_container(&mut br)
        .ok()
        .and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)))
        .unwrap_or(1)
}

fn decode(p: &Path, max: u32) -> Option<ColorImage> {
    let mut img = image::open(p).ok()?;
    if img.width() > max || img.height() > max {
        img = img.thumbnail(max, max);
    }
    img = match exif_orientation(p) {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    };
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

pub struct Loader {
    pub req_tx: Sender<Req>,
    res_rx: Receiver<Res>,
}

impl Loader {
    fn new(ctx: &Context) -> Self {
        let (req_tx, req_rx) = channel::<Req>();
        let (res_tx, res_rx) = channel::<Res>();
        let req_rx = Arc::new(Mutex::new(req_rx));
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
        for _ in 0..n {
            let (rx, tx, ctx) = (req_rx.clone(), res_tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let req = { rx.lock().unwrap().recv() };
                let Ok(req) = req else { break };
                let max = if req.kind == Kind::Thumb { 256 } else { 2400 };
                let img = decode(&req.path, max);
                if tx.send(Res { path: req.path, kind: req.kind, img }).is_err() {
                    break;
                }
                ctx.request_repaint();
            });
        }
        Loader { req_tx, res_rx }
    }
}

// ───────── 데이터 ─────────

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: SystemTime,
}

#[derive(Default)]
pub struct Info {
    pub rows: Vec<(String, String)>,
    pub exif: Vec<(String, String)>,
    pub dims: Option<(u32, u32)>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Browse,
    Organize,
    Bin,
}
#[derive(PartialEq, Clone, Copy)]
pub enum Sort {
    Name,
    Modified,
    Size,
}
#[derive(PartialEq, Clone, Copy)]
pub enum RunState {
    Idle,
    Running,
}
#[derive(PartialEq, Clone, Copy)]
pub enum ReportTab {
    Overview,
    Duplicates,
    Renamed,
}

pub struct QuickOrg {
    pub src: PathBuf,
    pub out: String,
    pub by_year: bool,
    pub trash_source: bool,
}

pub struct Root {
    pub label: String,
    pub path: PathBuf,
    pub icon: &'static str,
}

pub struct Org {
    pub src: String,
    pub out: String,
    pub by_year: bool,
    pub trash_source: bool,
    pub state: RunState,
    pub dry: bool,
    pub stage: usize,
    pub phase: String,
    pub progress: (usize, usize),
    pub live: Live,
    pub current: String,
    pub started: Instant,
    pub samples: Vec<f32>,
    pub last_sample: (Instant, usize),
    pub log: Vec<String>,
    pub report: Option<Report>,
    pub show_report: bool,
    pub report_tab: ReportTab,
    pub rx: Option<Receiver<Msg>>,
    pub cancel: Arc<AtomicBool>,
    pub finished_in: f32,
}

pub struct App {
    pub dark: bool,
    pub tab: Tab,
    pub loader: Loader,
    pub roots: Vec<Root>,
    pub kids: HashMap<PathBuf, Vec<PathBuf>>,
    pub expanded: HashSet<PathBuf>,
    pub selected_dir: Option<PathBuf>,
    pub pick: Option<PathBuf>,
    pub back: Vec<PathBuf>,
    pub entries: Vec<Entry>,
    pub recursive: bool,
    pub sort: Sort,
    pub ascending: bool,
    pub filter: String,
    pub thumb: f32,
    pub sel: HashSet<usize>,
    pub primary: Option<usize>,
    pub anchor: Option<usize>,
    pub cols: usize,
    pub thumbs: HashMap<PathBuf, Option<TextureHandle>>,
    pub pending: HashSet<PathBuf>,
    pub preview: Option<(PathBuf, Option<TextureHandle>)>,
    pub info: Info,
    pub hash: Option<String>,
    pub renaming: bool,
    pub rename_buf: String,
    pub viewer: bool,
    pub vzoom: f32,
    pub voff: Vec2,
    pub confirm_delete: bool,
    pub about: bool,
    pub toast: Option<(String, f64)>,
    pub focus_search: bool,
    pub org: Org,
    pub quick: Option<QuickOrg>,
    pub scroll_to: Option<usize>,
    pub scroll_tree: bool,
    pub bin_items: Vec<BinItem>,
    pub bin_sel: HashSet<String>,
    pub bin_dirty: bool,
    pub bin_confirm_empty: bool,
    pub update: updater::Shared,
    pub auto_update: bool,
}

fn settings_path() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|a| PathBuf::from(a).join("ImageCleaner").join("settings.ini"))
}

pub fn list_dirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            !n.starts_with('$') && n != "System Volume Information"
        })
        .map(|e| e.path())
        .collect();
    v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    v
}

pub fn fmt_time(t: std::io::Result<SystemTime>) -> String {
    t.map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| "-".into())
}

pub fn load_info(p: &Path) -> Info {
    let mut i = Info::default();
    let md = std::fs::metadata(p).ok();
    if let Some(m) = &md {
        i.rows.push(("크기".into(), format!("{} ({} bytes)", theme::fmt_size(m.len()), theme::fmt_num(m.len() as usize))));
    }
    if let Ok((w, h)) = image::image_dimensions(p) {
        i.dims = Some((w, h));
        i.rows.push(("해상도".into(), format!("{w} × {h}  ({:.1} MP)", (w as f64 * h as f64) / 1e6)));
    }
    if let Ok(f) = image::ImageReader::open(p).and_then(|r| r.with_guessed_format()) {
        if let Some(fmt) = f.format() {
            i.rows.push(("형식".into(), format!("{fmt:?}")));
        }
    }
    if let Some(m) = &md {
        i.rows.push(("생성일".into(), fmt_time(m.created())));
        i.rows.push(("수정일".into(), fmt_time(m.modified())));
    }
    i.rows.push(("위치".into(), p.parent().map(|s| s.display().to_string()).unwrap_or_default()));
    if let Ok(f) = std::fs::File::open(p) {
        let mut br = std::io::BufReader::new(f);
        if let Ok(ex) = exif::Reader::new().read_from_container(&mut br) {
            use exif::Tag as T;
            for (tag, label) in [
                (T::Make, "제조사"), (T::Model, "카메라"), (T::LensModel, "렌즈"),
                (T::DateTimeOriginal, "촬영일"), (T::ExposureTime, "노출 시간"), (T::FNumber, "조리개"),
                (T::PhotographicSensitivity, "ISO"), (T::FocalLength, "초점 거리"), (T::Flash, "플래시"),
                (T::Software, "소프트웨어"), (T::Orientation, "방향"),
                (T::GPSLatitude, "GPS 위도"), (T::GPSLongitude, "GPS 경도"),
            ] {
                if let Some(f) = ex.get_field(tag, exif::In::PRIMARY) {
                    i.exif.push((label.into(), f.display_value().with_unit(&ex).to_string()));
                }
            }
        }
    }
    i
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::setup_fonts(&cc.egui_ctx);
        let mut dark = !matches!(cc.egui_ctx.system_theme(), Some(Theme::Light));
        let mut roots = Vec::new();
        if let Ok(h) = std::env::var("USERPROFILE") {
            for (n, d, i) in [
                ("사진", "Pictures", ic::PHOTO), ("바탕 화면", "Desktop", ic::DESKTOP),
                ("다운로드", "Downloads", ic::DOWNLOAD), ("문서", "Documents", ic::DOC),
            ] {
                let p = PathBuf::from(&h).join(d);
                if p.is_dir() {
                    roots.push(Root { label: n.into(), path: p, icon: i });
                }
            }
        }
        for d in b'C'..=b'Z' {
            let p = PathBuf::from(format!("{}:\\", d as char));
            if p.exists() {
                roots.push(Root { label: format!("로컬 디스크 ({}:)", d as char), path: p, icon: ic::DRIVE });
            }
        }
        let mut org = Org {
            src: String::new(), out: String::new(), by_year: true, trash_source: false,
            state: RunState::Idle, dry: false, stage: 0, phase: String::new(), progress: (0, 0),
            live: Live::default(), current: String::new(), started: Instant::now(), samples: vec![],
            last_sample: (Instant::now(), 0), log: vec![], report: None, show_report: false,
            report_tab: ReportTab::Overview, rx: None, cancel: Arc::new(AtomicBool::new(false)),
            finished_in: 0.0,
        };
        let mut thumb = 150.0;
        let mut auto_update = true;
        let mut last_dir = None;
        if let Some(txt) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            for l in txt.trim_start_matches('\u{feff}').lines() {
                let Some((k, v)) = l.split_once('=') else { continue };
                match k {
                    "theme" => dark = v == "dark",
                    "src" => org.src = v.into(),
                    "out" => org.out = v.into(),
                    "by_year" => org.by_year = v == "1",
                    "trash" => org.trash_source = v == "1",
                    "auto_update" => auto_update = v == "1",
                    "thumb" => thumb = v.parse().unwrap_or(150.0),
                    "last_dir" => last_dir = Some(PathBuf::from(v)),
                    "root" => {
                        let p = PathBuf::from(v);
                        if p.is_dir() && !roots.iter().any(|r| r.path == p) {
                            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(v.into());
                            roots.insert(0, Root { label: n, path: p, icon: ic::FOLDER });
                        }
                    }
                    _ => {}
                }
            }
        }
        theme::apply_theme(&cc.egui_ctx, dark);
        let mut app = App {
            dark, tab: Tab::Browse, loader: Loader::new(&cc.egui_ctx), roots, kids: HashMap::new(),
            expanded: HashSet::new(), selected_dir: None, pick: None, back: vec![], entries: vec![],
            recursive: false, sort: Sort::Name, ascending: true, filter: String::new(), thumb,
            sel: HashSet::new(), primary: None, anchor: None, cols: 1, thumbs: HashMap::new(),
            pending: HashSet::new(), preview: None, info: Info::default(), hash: None, renaming: false,
            rename_buf: String::new(), viewer: false, vzoom: 1.0, voff: Vec2::ZERO,
            confirm_delete: false, about: false, toast: None, focus_search: false, org,
            quick: None, scroll_to: None, scroll_tree: false, bin_items: vec![], bin_sel: HashSet::new(), bin_dirty: true,
            bin_confirm_empty: false, update: Arc::new(Mutex::new(UpdateState::Idle)), auto_update,
        };
        if let Some(d) = last_dir.filter(|d| d.is_dir()) {
            app.navigate(d, false);
        }
        for a in std::env::args().skip(1) {
            match a.as_str() {
                "--tab=organize" => app.tab = Tab::Organize,
                "--tab=bin" => app.tab = Tab::Bin,
                "--select" => {
                    if !app.entries.is_empty() {
                        app.select_only(0);
                    }
                }
                "--no-update" => app.auto_update = false,
                "--quick" => {
                    if let Some(d) = app.selected_dir.clone() {
                        app.quick = Some(QuickOrg { out: crate::ui_organize::suggest_output(&d), src: d, by_year: true, trash_source: true });
                    }
                }
                "--run=dry" => app.start_organize(true),
                "--run=real" => app.start_organize(false),
                _ => {}
            }
        }
        if app.auto_update {
            updater::check(app.update.clone(), cc.egui_ctx.clone(), true);
        }
        app
    }

    pub fn save_settings(&self) {
        let Some(p) = settings_path() else { return };
        let mut s = format!(
            "theme={}\nsrc={}\nout={}\nby_year={}\ntrash={}\nauto_update={}\nthumb={}\n",
            if self.dark { "dark" } else { "light" }, self.org.src, self.org.out,
            self.org.by_year as u8, self.org.trash_source as u8, self.auto_update as u8, self.thumb
        );
        if let Some(d) = &self.selected_dir {
            s += &format!("last_dir={}\n", d.display());
        }
        for r in self.roots.iter().filter(|r| r.icon == ic::FOLDER) {
            s += &format!("root={}\n", r.path.display());
        }
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, s);
    }

    pub fn toast(&mut self, ctx: &Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time)));
    }

    pub fn children(&mut self, p: &Path) -> Vec<PathBuf> {
        self.kids.entry(p.to_path_buf()).or_insert_with(|| list_dirs(p)).clone()
    }

    /// 폴더 이동 (트리 확장 + 목록 로드)
    pub fn navigate(&mut self, dir: PathBuf, push_back: bool) {
        if push_back {
            if let Some(cur) = &self.selected_dir {
                self.back.push(cur.clone());
            }
        }
        for a in dir.ancestors().skip(1) {
            self.expanded.insert(a.to_path_buf());
        }
        self.selected_dir = Some(dir);
        self.scroll_tree = true;
        self.reload();
    }

    pub fn reload(&mut self) {
        self.entries.clear();
        self.thumbs.clear();
        self.pending.clear();
        self.sel.clear();
        self.primary = None;
        self.anchor = None;
        self.preview = None;
        self.info = Info::default();
        self.renaming = false;
        let Some(dir) = self.selected_dir.clone() else { return };
        let depth = if self.recursive { usize::MAX } else { 1 };
        for e in walkdir::WalkDir::new(&dir).max_depth(depth).into_iter().filter_map(|e| e.ok()) {
            if !e.file_type().is_file() || !organizer::is_image(e.path()) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            self.entries.push(Entry {
                name: e.file_name().to_string_lossy().into(),
                path: e.into_path(),
                size: m.len(),
                modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
            if self.entries.len() >= 30000 {
                break;
            }
        }
        self.sort_entries();
    }

    pub fn sort_entries(&mut self) {
        let asc = self.ascending;
        match self.sort {
            Sort::Name => self.entries.sort_by_key(|e| e.name.to_lowercase()),
            Sort::Modified => self.entries.sort_by_key(|e| e.modified),
            Sort::Size => self.entries.sort_by_key(|e| e.size),
        }
        if !asc {
            self.entries.reverse();
        }
        self.sel.clear();
        self.primary = None;
    }

    pub fn visible(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        (0..self.entries.len()).filter(|&i| f.is_empty() || self.entries[i].name.to_lowercase().contains(&f)).collect()
    }

    pub fn select_only(&mut self, idx: usize) {
        self.sel.clear();
        self.sel.insert(idx);
        self.anchor = Some(idx);
        self.set_primary(idx);
    }

    pub fn set_primary(&mut self, idx: usize) {
        self.primary = Some(idx);
        let p = self.entries[idx].path.clone();
        self.info = load_info(&p);
        self.hash = None;
        self.renaming = false;
        self.rename_buf = self.entries[idx].name.clone();
        self.vzoom = 1.0;
        self.voff = Vec2::ZERO;
        if !matches!(&self.preview, Some((q, _)) if *q == p) {
            self.preview = Some((p.clone(), None));
            let _ = self.loader.req_tx.send(Req { path: p, kind: Kind::Preview });
        }
    }

    pub fn poll_loader(&mut self, ctx: &Context) {
        while let Ok(r) = self.loader.res_rx.try_recv() {
            let tex = r.img.map(|img| ctx.load_texture(r.path.to_string_lossy(), img, TextureOptions::LINEAR));
            match r.kind {
                Kind::Thumb => {
                    if self.pending.remove(&r.path) {
                        self.thumbs.insert(r.path, tex);
                    }
                }
                Kind::Preview => {
                    if let Some((p, slot)) = &mut self.preview {
                        if *p == r.path {
                            *slot = tex;
                        }
                    }
                }
            }
        }
    }

    // ───── 정리 작업 ─────

    pub fn start_organize(&mut self, dry: bool) {
        let (tx, rx) = channel();
        let o = &mut self.org;
        o.rx = Some(rx);
        o.state = RunState::Running;
        o.dry = dry;
        o.stage = 0;
        o.log.clear();
        o.report = None;
        o.show_report = false;
        o.progress = (0, 0);
        o.live = Live::default();
        o.current.clear();
        o.samples.clear();
        o.started = Instant::now();
        o.last_sample = (Instant::now(), 0);
        o.cancel = Arc::new(AtomicBool::new(false));
        let opt = Options {
            source: PathBuf::from(&o.src),
            output: PathBuf::from(&o.out),
            by_year: o.by_year,
            trash_source: o.trash_source,
            dry_run: dry,
        };
        let c = o.cancel.clone();
        std::thread::spawn(move || organizer::run(opt, tx, c));
        self.save_settings();
    }

    pub fn poll_organizer(&mut self) {
        let o = &mut self.org;
        let Some(rx) = &o.rx else { return };
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Stage(s) => {
                    o.stage = s;
                    o.progress = (0, 0);
                }
                Msg::Current(s) => o.current = s,
                Msg::Live(l) => o.live = l,
                Msg::Phase(s) => {
                    if s.contains("휴지통") {
                        o.stage = 3;
                        o.progress = (0, 0);
                    }
                    o.phase = s;
                }
                Msg::Progress(a, b) => {
                    o.progress = (a, b);
                    let dt = o.last_sample.0.elapsed().as_secs_f32();
                    if dt >= 0.25 && a >= o.last_sample.1 {
                        o.samples.push((a - o.last_sample.1) as f32 / dt);
                        if o.samples.len() > 90 {
                            o.samples.remove(0);
                        }
                        o.last_sample = (Instant::now(), a);
                    }
                }
                Msg::Log(s) => {
                    if o.log.len() < 50000 {
                        o.log.push(s)
                    }
                }
                Msg::Done(r) => {
                    o.state = RunState::Idle;
                    o.finished_in = o.started.elapsed().as_secs_f32();
                    o.stage = 3;
                    o.live = Live {
                        scanned: r.scanned, duplicates: r.duplicates, renamed: r.renamed,
                        copied: r.copied, errors: r.errors, saved_bytes: r.saved_bytes,
                    };
                    o.show_report = !r.cancelled;
                    o.report_tab = ReportTab::Overview;
                    o.report = Some(r);
                    o.rx = None;
                    return;
                }
            }
        }
    }

    fn handle_drop(&mut self, ctx: &Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(p) = Some(f.path().to_path_buf()).filter(|p| p.is_dir()) {
                if self.tab == Tab::Organize {
                    self.org.src = p.display().to_string();
                } else {
                    if !self.roots.iter().any(|r| p.starts_with(&r.path)) {
                        let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        self.roots.insert(0, Root { label: n, path: p.clone(), icon: ic::FOLDER });
                    }
                    self.navigate(p, true);
                }
                self.save_settings();
                break;
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.poll_loader(ctx);
        self.poll_organizer();
        self.handle_drop(ctx);
        if self.org.state == RunState::Running {
            ctx.request_repaint();
        }
        let p = palette(self.dark);

        self.app_bar(ui, &p);
        if self.tab == Tab::Browse {
            self.path_bar(ui, &p);
        }
        self.update_banner(ui, &p);
        self.status_bar(ui, &p);
        if self.tab == Tab::Browse {
            self.sidebar(ui, &p);
            self.inspector(ui, &p);
            CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(Margin::same(10))).show(ui, |ui| self.grid(ui, &p));
        } else if self.tab == Tab::Organize {
            CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(Margin::same(0))).show(ui, |ui| self.organize_page(ui, &p));
        } else {
            CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(Margin::same(0))).show(ui, |ui| self.bin_page(ui, &p));
        }
        self.overlays(ctx, &p);

        if let Some(d) = self.pick.take() {
            self.navigate(d, true);
            self.save_settings();
        }
    }
}
