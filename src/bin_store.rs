// SPDX-License-Identifier: GPL-3.0-or-later
//! 앱 내 휴지통. %APPDATA%\ImageCleaner\Trash 에 파일을 보관하고
//! index.tsv 에 (id, 원래 경로, 삭제 시각, 크기)를 기록해 복원/영구삭제/비우기를 지원한다.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct BinItem {
    pub id: String,
    pub original: PathBuf,
    pub deleted_at: u64, // unix 초
    pub size: u64,
    pub stored: PathBuf,
}

pub fn root() -> PathBuf {
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    base.join("ImageCleaner").join("Trash")
}

fn index_path() -> PathBuf {
    root().join("index.tsv")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

pub fn list() -> Vec<BinItem> {
    let Ok(txt) = fs::read_to_string(index_path()) else { return vec![] };
    let mut v: Vec<BinItem> = txt
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let id = it.next()?.to_string();
            let original = PathBuf::from(it.next()?);
            let deleted_at = it.next()?.parse().ok()?;
            let size = it.next()?.parse().ok()?;
            let stored = root().join(&id);
            stored.is_file().then_some(BinItem { id, original, deleted_at, size, stored })
        })
        .collect();
    v.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    v
}

fn write_index(items: &[BinItem]) -> std::io::Result<()> {
    fs::create_dir_all(root())?;
    let s: String = items
        .iter()
        .map(|i| format!("{}\t{}\t{}\t{}\n", i.id, i.original.display(), i.deleted_at, i.size))
        .collect();
    fs::write(index_path(), s)
}

/// 파일을 앱 휴지통으로 이동(원본 위치 기록).
pub fn move_in(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root())?;
    let size = fs::metadata(path)?.len();
    let t = now();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut n = 0u32;
    let id = loop {
        let cand = format!("{t}_{n}_{name}");
        if !root().join(&cand).exists() {
            break cand;
        }
        n += 1;
    };
    move_file(path, &root().join(&id))?;
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let _ = abs;
    let mut items = list();
    items.push(BinItem { id: id.clone(), original: path.to_path_buf(), deleted_at: t, size, stored: root().join(&id) });
    write_index(&items)
}

/// 여러 파일을 한 번에 이동 (인덱스 기록은 한 번만).
pub fn move_in_many(paths: &[PathBuf]) -> (usize, Vec<String>) {
    let mut ok = 0;
    let mut errs = vec![];
    for p in paths {
        match move_in(p) {
            Ok(_) => ok += 1,
            Err(e) => errs.push(format!("{}: {e}", p.display())),
        }
    }
    (ok, errs)
}

/// 원래 위치로 복원. 같은 이름이 있으면 "이름 (복원 n)" 으로 저장. 복원된 경로를 반환.
pub fn restore(id: &str) -> std::io::Result<PathBuf> {
    let mut items = list();
    let Some(pos) = items.iter().position(|i| i.id == id) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "항목을 찾을 수 없습니다"));
    };
    let it = items.remove(pos);
    if let Some(dir) = it.original.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut dest = it.original.clone();
    let mut n = 1;
    while dest.exists() {
        let stem = it.original.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = it.original.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        dest = it.original.with_file_name(format!("{stem} (복원 {n}){ext}"));
        n += 1;
    }
    move_file(&it.stored, &dest)?;
    write_index(&items)?;
    Ok(dest)
}

pub fn purge(id: &str) -> std::io::Result<()> {
    let mut items = list();
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let it = items.remove(pos);
        let _ = fs::remove_file(&it.stored);
        write_index(&items)?;
    }
    Ok(())
}

pub fn empty() -> std::io::Result<usize> {
    let items = list();
    let n = items.len();
    for it in &items {
        let _ = fs::remove_file(&it.stored);
    }
    write_index(&[])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_roundtrip() {
        let d = std::env::temp_dir().join("ic_bin_test");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("a.png");
        fs::write(&f, b"hello").unwrap();
        move_in(&f).unwrap();
        assert!(!f.exists());
        let it = list().into_iter().find(|i| i.original == f).expect("항목 존재");
        fs::write(&f, b"new").unwrap(); // 같은 이름 충돌
        let r = restore(&it.id).unwrap();
        assert_ne!(r, f);
        assert_eq!(fs::read(&r).unwrap(), b"hello");
        assert!(list().iter().all(|i| i.original != f));
    }
}
