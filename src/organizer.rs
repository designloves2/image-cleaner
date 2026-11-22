// SPDX-License-Identifier: GPL-3.0-or-later
//! 스캔 → 완전 중복 제거 → 이름 충돌 해결 → (선택) 연도별 폴더로 병합하는 핵심 로직.

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use walkdir::WalkDir;

pub const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "ico", "tga", "heic", "heif", "avif",
    "jfif", "dng", "cr2", "nef", "arw",
];

pub fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct Options {
    pub source: PathBuf,
    pub output: PathBuf,
    pub by_year: bool,
    /// true 면 정리 후 원본을 앱 내 휴지통으로 보관(복원 가능), false 면 원본 유지
    pub trash_source: bool,
    /// true 면 모든 이미지 파일명 뒤에 원본 폴더명을 접미사로 붙인다 (예: IMG_001_여행.jpg)
    pub suffix_all: bool,
    pub dry_run: bool,
}

#[derive(Default, Clone)]
pub struct Report {
    pub dry_run: bool,
    pub trashed: usize,
    pub years: std::collections::BTreeMap<String, usize>,
    /// (중복 파일, 남겨둘 원본)
    pub dup_list: Vec<(PathBuf, PathBuf)>,
    /// (원본 경로, 새 파일명)
    pub rename_list: Vec<(PathBuf, String)>,
    pub scanned: usize,
    pub duplicates: usize,
    pub renamed: usize,
    pub copied: usize,
    pub errors: usize,
    pub saved_bytes: u64,
    pub cancelled: bool,
}

#[derive(Default, Clone, Copy)]
pub struct Live {
    pub scanned: usize,
    pub duplicates: usize,
    pub renamed: usize,
    pub copied: usize,
    pub errors: usize,
    pub saved_bytes: u64,
}

pub enum Msg {
    Stage(usize), // 0 스캔, 1 중복 검사, 2 병합
    Current(String),
    Live(Live),
    Phase(String),
    Progress(usize, usize),
    Log(String),
    Done(Report),
}

pub fn hash_file(p: &Path) -> std::io::Result<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    h.update_reader(File::open(p)?)?;
    Ok(*h.finalize().as_bytes())
}

/// 이미지가 최초 생성된 연도 (EXIF 촬영일 → 파일 생성/수정일 중 이른 값).
pub fn creation_year(p: &Path) -> Option<i32> {
    use chrono::Datelike;
    if let Ok(f) = File::open(p) {
        let mut br = std::io::BufReader::new(f);
        if let Ok(ex) = exif::Reader::new().read_from_container(&mut br) {
            for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized, exif::Tag::DateTime] {
                if let Some(f) = ex.get_field(tag, exif::In::PRIMARY) {
                    let s = f.display_value().to_string();
                    if let Some(y) = s.get(0..4).and_then(|y| y.parse::<i32>().ok()) {
                        if (1900..=2200).contains(&y) {
                            return Some(y);
                        }
                    }
                }
            }
        }
    }
    let md = fs::metadata(p).ok()?;
    let t = [md.created().ok(), md.modified().ok()].into_iter().flatten().min()?;
    let dt: chrono::DateTime<chrono::Local> = t.into();
    Some(dt.year())
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn sanitize(s: &str) -> String {
    let t: String = s.chars().map(|c| if ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) || c.is_control() { '_' } else { c }).collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() { "폴더".into() } else { t }
}

/// 이미지가 들어 있던 폴더명(바깥→안쪽, 최대 `level`개)을 "_"로 이은 접미사.
fn folder_label(src: &Path, level: usize) -> String {
    let parts: Vec<String> = src
        .parent()
        .map(|p| p.components().filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(sanitize(&n.to_string_lossy())),
            _ => None,
        }).collect())
        .unwrap_or_default();
    if parts.is_empty() {
        return "root".into();
    }
    parts[parts.len().saturating_sub(level)..].join("_")
}

fn suffixed_name(orig: &str, src: &Path, level: usize) -> String {
    if level == 0 {
        return orig.to_string();
    }
    let (stem, ext) = split_name(orig);
    format!("{stem}_{}{ext}", folder_label(src, level))
}

/// 출력 폴더에서 실제로 쓸 경로를 정한다. Ok(None) => 출력 폴더에 이미 동일 파일이 있음.
/// 이름이 겹치고 내용이 다르면 `alt`(폴더명 접미사 이름) → `_2`, `_3` … 순으로 시도한다.
fn unique_dest(dir: &Path, name: &str, src: &Path, src_hash: &mut Option<[u8; 32]>, planned: &HashSet<PathBuf>, alt: Option<&str>) -> Result<Option<(PathBuf, bool)>, std::io::Error> {
    let mut names: Vec<String> = vec![name.to_string()];
    if let Some(a) = alt {
        names.push(a.to_string());
    }
    let mut i = 0;
    loop {
        let nm = if i < names.len() {
            names[i].clone()
        } else {
            let (stem, ext) = split_name(names.last().unwrap());
            format!("{stem}_{}{ext}", i - names.len() + 2)
        };
        let cand = dir.join(&nm);
        i += 1;
        if planned.contains(&cand) {
            continue;
        }
        if !cand.exists() {
            return Ok(Some((cand, i > 1)));
        }
        if fs::metadata(&cand)?.len() == fs::metadata(src)?.len() {
            if src_hash.is_none() {
                *src_hash = Some(hash_file(src)?);
            }
            if hash_file(&cand)? == src_hash.unwrap() {
                return Ok(None);
            }
        }
    }
}

pub fn run(opt: Options, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    let mut rep = Report { dry_run: opt.dry_run, ..Default::default() };
    let mut planned: HashSet<PathBuf> = HashSet::new();
    let send = |m: Msg| {
        let _ = tx.send(m);
    };

    // 1. 스캔
    send(Msg::Stage(0));
    send(Msg::Phase("폴더 스캔 중...".into()));
    let out_abs = fs::canonicalize(&opt.output).unwrap_or(opt.output.clone());
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    for e in WalkDir::new(&opt.source).into_iter().filter_map(|e| e.ok()) {
        if cancel.load(Ordering::Relaxed) {
            rep.cancelled = true;
            send(Msg::Done(rep));
            return;
        }
        if !e.file_type().is_file() || !is_image(e.path()) {
            continue;
        }
        if fs::canonicalize(e.path()).map(|p| p.starts_with(&out_abs)).unwrap_or(false) {
            continue; // 출력 폴더 안의 파일은 제외
        }
        let len = e.metadata().map(|m| m.len()).unwrap_or(0);
        files.push((e.into_path(), len));
        if files.len() % 64 == 0 {
            send(Msg::Live(Live { scanned: files.len(), ..Default::default() }));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    rep.scanned = files.len();
    send(Msg::Log(format!("이미지 {}개 발견", files.len())));

    // 2. 크기가 같은 파일만 해시 비교 → 완전 중복 판정
    send(Msg::Stage(1));
    send(Msg::Live(Live { scanned: files.len(), ..Default::default() }));
    send(Msg::Phase("중복 검사 중...".into()));
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, (_, s)) in files.iter().enumerate() {
        by_size.entry(*s).or_default().push(i);
    }
    let cand: Vec<usize> = by_size.values().filter(|v| v.len() > 1).flatten().copied().collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = cand.len();
    let hashes: HashMap<usize, [u8; 32]> = cand
        .par_iter()
        .filter_map(|&i| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let h = hash_file(&files[i].0).ok();
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 8 == 0 || d == total {
                let _ = tx.send(Msg::Progress(d, total));
            }
            h.map(|h| (i, h))
        })
        .collect();
    if cancel.load(Ordering::Relaxed) {
        rep.cancelled = true;
        send(Msg::Done(rep));
        return;
    }
    let mut seen: HashMap<(u64, [u8; 32]), usize> = HashMap::new();
    let mut keep: Vec<usize> = Vec::new();
    let mut dup_orig: Vec<(usize, usize)> = Vec::new();
    let mut kept_ok: HashSet<usize> = HashSet::new();
    for i in 0..files.len() {
        match hashes.get(&i) {
            Some(h) => {
                match seen.get(&(files[i].1, *h)) {
                    None => {
                        seen.insert((files[i].1, *h), i);
                        keep.push(i);
                    }
                    Some(&orig) => {
                        dup_orig.push((i, orig));
                        rep.duplicates += 1;
                        rep.saved_bytes += files[i].1;
                        if rep.dup_list.len() < 5000 {
                            rep.dup_list.push((files[i].0.clone(), files[orig].0.clone()));
                        }
                        send(Msg::Log(format!("중복 제외: {}", files[i].0.display())));
                    }
                }
            }
            None => keep.push(i),
        }
    }

    // 3. 병합
    send(Msg::Stage(2));
    send(Msg::Phase(if opt.dry_run { "결과 분석 중..." } else { "복사 중..." }.into()));
    if !opt.dry_run {
        if let Err(e) = fs::create_dir_all(&opt.output) {
            send(Msg::Log(format!("출력 폴더 생성 실패: {e}")));
            rep.errors += 1;
            send(Msg::Done(rep));
            return;
        }
    }
    // 파일명 계획: 같은 폴더에 같은 이름이 겹치면 겹친 파일 "모두" 폴더명 접미사로 구분한다.
    send(Msg::Phase("파일명 계획 중...".into()));
    let dirs: Vec<PathBuf> = keep
        .par_iter()
        .map(|&i| {
            if opt.by_year {
                match creation_year(&files[i].0) {
                    Some(y) => opt.output.join(y.to_string()),
                    None => opt.output.join("연도미상"),
                }
            } else {
                opt.output.clone()
            }
        })
        .collect();
    let orig_names: Vec<String> = keep
        .iter()
        .map(|&i| files[i].0.file_name().and_then(|s| s.to_str()).unwrap_or("image").to_string())
        .collect();
    let mut level: Vec<usize> = vec![0; keep.len()];
    {
        let mut groups: HashMap<(PathBuf, String), Vec<usize>> = HashMap::new();
        for k in 0..keep.len() {
            groups.entry((dirs[k].clone(), orig_names[k].to_lowercase())).or_default().push(k);
        }
        for members in groups.values() {
            if members.len() > 1 || opt.suffix_all {
                for &m in members {
                    level[m] = 1;
                }
            }
        }
        // 접미사가 같아 다시 겹치면 상위 폴더명을 더 붙여 구분
        for _ in 0..8 {
            let mut seen: HashMap<(PathBuf, String), Vec<usize>> = HashMap::new();
            for k in 0..keep.len() {
                let nm = suffixed_name(&orig_names[k], &files[keep[k]].0, level[k]).to_lowercase();
                seen.entry((dirs[k].clone(), nm)).or_default().push(k);
            }
            let mut changed = false;
            for v in seen.values().filter(|v| v.len() > 1) {
                for &m in v {
                    level[m] += 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    let plan: Vec<(PathBuf, String, Option<String>)> = (0..keep.len())
        .map(|k| {
            let src = &files[keep[k]].0;
            let name = suffixed_name(&orig_names[k], src, level[k]);
            let alt = if level[k] == 0 { Some(suffixed_name(&orig_names[k], src, 1)) } else { None };
            (dirs[k].clone(), name, alt)
        })
        .collect();
    send(Msg::Phase(if opt.dry_run { "결과 분석 중..." } else { "복사 중..." }.into()));
    let total = keep.len();
    for (n, &i) in keep.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            rep.cancelled = true;
            break;
        }
        if let Some(ms) = std::env::var("IC_SLOW_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
            std::thread::sleep(std::time::Duration::from_millis(ms)); // 개발용: 처리 화면 확인
        }
        let src = &files[i].0;
        let _ = tx.send(Msg::Current(src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        let (dir, name, alt) = plan[n].clone();
        let year_label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let orig_name = orig_names[n].clone();
        let mut h = hashes.get(&i).copied();
        let res: std::io::Result<()> = (|| {
            if !opt.dry_run {
                fs::create_dir_all(&dir)?;
            }
            match unique_dest(&dir, &name, src, &mut h, &planned, alt.as_deref())? {
                None => {
                    rep.duplicates += 1;
                    rep.saved_bytes += files[i].1;
                    let _ = tx.send(Msg::Log(format!("출력 폴더에 이미 존재: {}", src.display())));
                    kept_ok.insert(i);
                }
                Some((dest, _)) => {
                    let renamed = dest.file_name().map(|f| f.to_string_lossy() != orig_name.as_str()).unwrap_or(false);
                    *rep.years.entry(if opt.by_year { year_label.clone() } else { "전체".into() }).or_default() += 1;
                    if renamed {
                        rep.renamed += 1;
                        if rep.rename_list.len() < 5000 {
                            rep.rename_list.push((src.clone(), dest.file_name().unwrap().to_string_lossy().to_string()));
                        }
                        let _ = tx.send(Msg::Log(format!(
                            "이름 변경: {} → {}",
                            orig_name,
                            dest.file_name().unwrap().to_string_lossy()
                        )));
                    }
                    if opt.dry_run {
                        planned.insert(dest);
                    } else {
                        fs::copy(src, &dest)?;
                    }
                    kept_ok.insert(i);
                    rep.copied += 1;
                }
            }
            Ok(())
        })();
        if let Err(e) = res {
            rep.errors += 1;
            send(Msg::Log(format!("오류: {} ({e})", src.display())));
        }
        if n % 4 == 0 || n + 1 == total {
            send(Msg::Progress(n + 1, total));
            send(Msg::Live(Live {
                scanned: rep.scanned, duplicates: rep.duplicates, renamed: rep.renamed,
                copied: rep.copied, errors: rep.errors, saved_bytes: rep.saved_bytes,
            }));
        }
    }
    // 4. 원본 처리: 복사가 확인된 파일(과 그 중복본)만 앱 내 휴지통으로 보관
    if opt.trash_source && !opt.dry_run && !rep.cancelled {
        send(Msg::Phase("원본을 휴지통으로 보관 중...".into()));
        let mut targets: Vec<usize> = kept_ok.iter().copied().collect();
        targets.extend(dup_orig.iter().filter(|(_, o)| kept_ok.contains(o)).map(|(d, _)| *d));
        targets.sort_unstable();
        let total = targets.len();
        for (n, &i) in targets.iter().enumerate() {
            match crate::bin_store::move_in(&files[i].0) {
                Ok(_) => rep.trashed += 1,
                Err(e) => {
                    rep.errors += 1;
                    send(Msg::Log(format!("원본 보관 실패: {} ({e})", files[i].0.display())));
                }
            }
            if n % 4 == 0 || n + 1 == total {
                send(Msg::Progress(n + 1, total));
            }
        }
        // 비어 버린 하위 폴더 정리 (루트는 유지)
        let mut dirs: Vec<PathBuf> = WalkDir::new(&opt.source).min_depth(1).into_iter().filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir()).map(|e| e.into_path()).collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        for d in dirs {
            let _ = fs::remove_dir(&d);
        }
    }
    send(Msg::Done(rep));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(p: &Path, c: u8) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(8, 8, image::Rgb([c, c, c])).save(p).unwrap();
    }

    #[test]
    fn dedupe_rename_year() {
        let base = std::env::temp_dir().join("ic_test");
        let _ = fs::remove_dir_all(&base);
        let (s, o) = (base.join("src"), base.join("out"));
        png(&s.join("a/x.png"), 10);
        png(&s.join("b/x_copy.png"), 10); // x.png 와 완전 동일 → 중복
        png(&s.join("c/x.png"), 200); // 이름만 같고 내용 다름 → 이름 변경
        png(&s.join("c/y.png"), 90);
        let (tx, rx) = std::sync::mpsc::channel();
        run(Options { source: s.clone(), output: o.clone(), by_year: true, trash_source: false, suffix_all: false, dry_run: false },
            tx, Arc::new(AtomicBool::new(false)));
        let rep = rx.iter().find_map(|m| if let Msg::Done(r) = m { Some(r) } else { None }).unwrap();
        assert_eq!((rep.scanned, rep.duplicates, rep.renamed, rep.copied, rep.errors), (4, 1, 2, 3, 0));
        let year = chrono::Local::now().format("%Y").to_string();
        let mut names: Vec<String> = fs::read_dir(o.join(&year)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        // 이름이 겹친 두 파일 모두 폴더명 접미사로 구분 (괄호 번호 없음)
        assert_eq!(names, vec!["x_a.png", "x_c.png", "y.png"]);
        // 재실행해도 이미 있는 동일 파일은 중복 처리되어 늘어나지 않음
        let (tx, rx) = std::sync::mpsc::channel();
        run(Options { source: s, output: o.clone(), by_year: true, trash_source: false, suffix_all: false, dry_run: false }, tx, Arc::new(AtomicBool::new(false)));
        let _ = rx.iter().count();
        assert_eq!(fs::read_dir(o.join(&year)).unwrap().count(), 3);
    }

    #[test]
    fn trash_source_moves_originals_to_app_bin() {
        let base = std::env::temp_dir().join("ic_test_trash");
        let _ = fs::remove_dir_all(&base);
        let (s, o) = (base.join("src"), base.join("out"));
        png(&s.join("a/x.png"), 10);
        png(&s.join("b/x_copy.png"), 10); // 중복
        png(&s.join("c/y.png"), 90);
        let (tx, rx) = std::sync::mpsc::channel();
        run(Options { source: s.clone(), output: o.clone(), by_year: false, trash_source: true, suffix_all: false, dry_run: false },
            tx, Arc::new(AtomicBool::new(false)));
        let rep = rx.iter().find_map(|m| if let Msg::Done(r) = m { Some(r) } else { None }).unwrap();
        assert_eq!((rep.copied, rep.duplicates, rep.trashed, rep.errors), (2, 1, 3, 0));
        assert!(!s.join("a/x.png").exists() && !s.join("b/x_copy.png").exists());
        assert_eq!(fs::read_dir(&o).unwrap().count(), 2);
        // 앱 휴지통에서 복원 가능해야 함
        let items: Vec<_> = crate::bin_store::list().into_iter().filter(|i| i.original.starts_with(&s)).collect();
        assert_eq!(items.len(), 3);
        for it in &items {
            crate::bin_store::restore(&it.id).unwrap();
        }
        assert!(s.join("a/x.png").exists() && s.join("c/y.png").exists());
    }

    fn run_opts(s: &Path, o: &Path, suffix_all: bool) -> Report {
        let (tx, rx) = std::sync::mpsc::channel();
        run(Options { source: s.to_path_buf(), output: o.to_path_buf(), by_year: false, trash_source: false, suffix_all, dry_run: false },
            tx, Arc::new(AtomicBool::new(false)));
        rx.iter().find_map(|m| if let Msg::Done(r) = m { Some(r) } else { None }).unwrap()
    }

    fn names_in(d: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        v.sort();
        v
    }

    #[test]
    fn suffix_all_adds_folder_name_to_every_file() {
        let base = std::env::temp_dir().join("ic_test_suffix");
        let _ = fs::remove_dir_all(&base);
        let (s, o) = (base.join("src"), base.join("out"));
        png(&s.join("여행/p.png"), 10);
        png(&s.join("가족/q.png"), 90);
        let rep = run_opts(&s, &o, true);
        assert_eq!((rep.copied, rep.renamed, rep.errors), (2, 2, 0));
        assert_eq!(names_in(&o), vec!["p_여행.png", "q_가족.png"]);
    }

    #[test]
    fn same_parent_name_gets_longer_suffix_no_parentheses() {
        let base = std::env::temp_dir().join("ic_test_samepar");
        let _ = fs::remove_dir_all(&base);
        let (s, o) = (base.join("src"), base.join("out"));
        png(&s.join("1/x/p.png"), 10);
        png(&s.join("2/x/p.png"), 200);
        let rep = run_opts(&s, &o, false);
        assert_eq!((rep.copied, rep.errors), (2, 0));
        let n = names_in(&o);
        assert_eq!(n, vec!["p_1_x.png", "p_2_x.png"]);
        assert!(n.iter().all(|x| !x.contains('(')));
    }
}
