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

fn unique_dest(dir: &Path, name: &str, src: &Path, src_hash: &mut Option<[u8; 32]>, planned: &HashSet<PathBuf>) -> Result<Option<(PathBuf, bool)>, std::io::Error> {
    // Ok(None) => 출력 폴더에 이미 동일 파일이 있음
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut n = 0;
    loop {
        let cand = if n == 0 { dir.join(name) } else { dir.join(format!("{stem} ({n}){ext}")) };
        if planned.contains(&cand) {
            n += 1;
            continue;
        }
        if !cand.exists() {
            return Ok(Some((cand, n > 0)));
        }
        if fs::metadata(&cand)?.len() == fs::metadata(src)?.len() {
            if src_hash.is_none() {
                *src_hash = Some(hash_file(src)?);
            }
            if hash_file(&cand)? == src_hash.unwrap() {
                return Ok(None);
            }
        }
        n += 1;
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
        let dir = if opt.by_year {
            match creation_year(src) {
                Some(y) => opt.output.join(y.to_string()),
                None => opt.output.join("연도미상"),
            }
        } else {
            opt.output.clone()
        };
        let year_label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("image").to_string();
        let mut h = hashes.get(&i).copied();
        let res: std::io::Result<()> = (|| {
            if !opt.dry_run {
                fs::create_dir_all(&dir)?;
            }
            match unique_dest(&dir, &name, src, &mut h, &planned)? {
                None => {
                    rep.duplicates += 1;
                    rep.saved_bytes += files[i].1;
                    let _ = tx.send(Msg::Log(format!("출력 폴더에 이미 존재: {}", src.display())));
                    kept_ok.insert(i);
                }
                Some((dest, renamed)) => {
                    *rep.years.entry(if opt.by_year { year_label.clone() } else { "전체".into() }).or_default() += 1;
                    if renamed {
                        rep.renamed += 1;
                        if rep.rename_list.len() < 5000 {
                            rep.rename_list.push((src.clone(), dest.file_name().unwrap().to_string_lossy().to_string()));
                        }
                        let _ = tx.send(Msg::Log(format!(
                            "이름 변경: {} → {}",
                            name,
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
        run(Options { source: s.clone(), output: o.clone(), by_year: true, trash_source: false, dry_run: false },
            tx, Arc::new(AtomicBool::new(false)));
        let rep = rx.iter().find_map(|m| if let Msg::Done(r) = m { Some(r) } else { None }).unwrap();
        assert_eq!((rep.scanned, rep.duplicates, rep.renamed, rep.copied, rep.errors), (4, 1, 1, 3, 0));
        let year = chrono::Local::now().format("%Y").to_string();
        let n = fs::read_dir(o.join(&year)).unwrap().count();
        assert_eq!(n, 3);
        // 재실행해도 이미 있는 동일 파일은 중복 처리되어 늘어나지 않음
        let (tx, rx) = std::sync::mpsc::channel();
        run(Options { source: s, output: o.clone(), by_year: true, trash_source: false, dry_run: false }, tx, Arc::new(AtomicBool::new(false)));
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
        run(Options { source: s.clone(), output: o.clone(), by_year: false, trash_source: true, dry_run: false },
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
}
