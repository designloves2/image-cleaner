// SPDX-License-Identifier: GPL-3.0-or-later
//! 앱 내 휴지통. %APPDATA%\ImageCleaner\Trash 에 파일을 보관하고
//! index.tsv 에 (id, 원래 경로, 삭제 시각, 크기)를 기록해 복원/영구삭제/비우기를 지원한다.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 색인(index.tsv) 읽기-수정-쓰기를 직렬화한다 (삭제와 정리가 동시에 일어나도 항목이 유실되지 않도록).
static INDEX_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct BinItem {
    pub id: String,
    pub original: PathBuf,
    pub deleted_at: u64, // unix 초
    pub size: u64,
    pub stored: PathBuf,
}

#[cfg(test)]
pub static TEST_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 테스트가 실제 앱 휴지통을 건드리지 않도록 임시 폴더를 휴지통 위치로 쓴다.
#[cfg(test)]
pub fn init_test_root() {
    TEST_ROOT.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("ic_trash_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    });
}

pub fn root() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = TEST_ROOT.get() {
        return p.clone();
    }
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
    let tmp = root().join("index.tsv.tmp");
    fs::write(&tmp, s)?;
    fs::rename(&tmp, index_path())
}

/// 파일을 앱 휴지통으로 이동(원본 위치 기록).
pub fn move_in(path: &Path) -> std::io::Result<()> {
    let _g = lock();
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

/// 복원 결과
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// 원래 이름 그대로 복원
    Restored(PathBuf),
    /// 같은 이름의 다른 파일이 있어 `이름_복원.ext` 로 복원
    Renamed(PathBuf),
    /// 내용이 같은 파일이 이미 있어 휴지통에서만 제거
    AlreadyThere(PathBuf),
    /// 원래 위치를 쓸 수 없어 다른 위치로 복원
    Elsewhere(PathBuf),
}

#[derive(Default, Debug)]
pub struct Summary {
    pub restored: usize,
    pub renamed: usize,
    pub already: usize,
    pub elsewhere: usize,
    pub failed: Vec<String>,
}

impl Summary {
    fn add(&mut self, o: &Outcome) {
        match o {
            Outcome::Restored(_) => self.restored += 1,
            Outcome::Renamed(_) => self.renamed += 1,
            Outcome::AlreadyThere(_) => self.already += 1,
            Outcome::Elsewhere(_) => self.elsewhere += 1,
        }
    }
    pub fn text(&self) -> String {
        let mut p = vec![];
        if self.restored > 0 { p.push(format!("{}개 복원", self.restored)); }
        if self.renamed > 0 { p.push(format!("{}개는 같은 이름이 있어 '_복원' 이름으로 복원", self.renamed)); }
        if self.already > 0 { p.push(format!("{}개는 동일한 파일이 이미 있어 휴지통에서만 제거", self.already)); }
        if self.elsewhere > 0 { p.push(format!("{}개는 다른 위치로 복원", self.elsewhere)); }
        if !self.failed.is_empty() { p.push(format!("{}개 실패", self.failed.len())); }
        if p.is_empty() { "복원할 항목이 없습니다".into() } else { p.join(" · ") }
    }
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// 보관 중인 파일을 `dir` 로 옮긴다. 같은 이름이 있으면 내용을 비교해 처리한다.
fn place(it: &BinItem, dir: &Path) -> std::io::Result<Outcome> {
    fs::create_dir_all(dir)?;
    let name = it.original.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
    let dest = dir.join(&name);
    if !dest.exists() {
        move_file(&it.stored, &dest)?;
        return Ok(Outcome::Restored(dest));
    }
    if fs::metadata(&dest)?.len() == it.size
        && crate::organizer::hash_file(&dest)? == crate::organizer::hash_file(&it.stored)?
    {
        let _ = fs::remove_file(&it.stored);
        return Ok(Outcome::AlreadyThere(dest));
    }
    let (stem, ext) = split_name(&name);
    let mut n = 1;
    loop {
        let nm = if n == 1 { format!("{stem}_복원{ext}") } else { format!("{stem}_복원{n}{ext}") };
        let cand = dir.join(nm);
        if !cand.exists() {
            move_file(&it.stored, &cand)?;
            return Ok(Outcome::Renamed(cand));
        }
        n += 1;
    }
}

fn clean(s: &str) -> String {
    let t: String = s.chars().map(|c| if ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) { '_' } else { c }).collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() { "폴더".into() } else { t }
}

fn comps(p: &Path) -> Vec<String> {
    p.components().filter_map(|c| match c {
        std::path::Component::Normal(n) => Some(clean(&n.to_string_lossy())),
        std::path::Component::Prefix(x) => Some(clean(&x.as_os_str().to_string_lossy())),
        _ => None,
    }).collect()
}

fn fallback_dir(it: &BinItem) -> PathBuf {
    let base = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let name = it.original.parent().and_then(|p| p.file_name()).map(|n| clean(&n.to_string_lossy())).unwrap_or_else(|| "복원".into());
    base.join("Pictures").join("Image Cleaner 복원").join(name)
}

/// 원래 위치로 복원. 원래 위치를 쓸 수 없으면(드라이브/폴더 생성 불가) 대체 위치로 복원한다.
pub fn restore(id: &str) -> std::io::Result<Outcome> {
    let _g = lock();
    let mut items = list();
    let Some(pos) = items.iter().position(|i| i.id == id) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "항목을 찾을 수 없습니다"));
    };
    let it = items.remove(pos);
    let parent = it.original.parent().map(|p| p.to_path_buf());
    let root_ok = parent.as_ref().and_then(|p| p.ancestors().last().map(|r| r.as_os_str().is_empty() || r.exists())).unwrap_or(false);
    let out = match parent.filter(|_| root_ok).map(|p| place(&it, &p)) {
        Some(Ok(o)) => o,
        _ => match place(&it, &fallback_dir(&it))? {
            Outcome::Restored(p) | Outcome::Renamed(p) | Outcome::Elsewhere(p) => Outcome::Elsewhere(p),
            other => other,
        },
    };
    write_index(&items)?;
    Ok(out)
}

pub fn restore_many(ids: &[String]) -> Summary {
    let mut s = Summary::default();
    for id in ids {
        match restore(id) {
            Ok(o) => s.add(&o),
            Err(e) => s.failed.push(format!("{id}: {e}")),
        }
    }
    s
}

/// 선택한 폴더 아래로 복원. 원래 폴더명으로 하위 폴더를 만들고,
/// 서로 다른 곳에서 온 같은 이름의 폴더(A\여행, B\여행)는 상위 폴더명을 붙여 구분한다(A_여행, B_여행).
pub fn restore_to(ids: &[String], dest_root: &Path) -> Summary {
    use std::collections::{HashMap, HashSet};
    let _g = lock();
    let mut items = list();
    let mut s = Summary::default();
    let sel: Vec<BinItem> = items.iter().filter(|i| ids.contains(&i.id)).cloned().collect();
    let mut by_name: HashMap<String, HashSet<PathBuf>> = HashMap::new();
    for it in &sel {
        let parent = it.original.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let nm = comps(&parent).last().cloned().unwrap_or_else(|| "복원".into());
        by_name.entry(nm).or_default().insert(parent);
    }
    let label = |parent: &Path| -> String {
        let c = comps(parent);
        let nm = c.last().cloned().unwrap_or_else(|| "복원".into());
        let group = &by_name[&nm];
        if group.len() <= 1 {
            return nm;
        }
        for k in 1..=c.len().max(1) {
            let l = |p: &Path| {
                let c = comps(p);
                c[c.len().saturating_sub(k)..].join("_")
            };
            let mine = l(parent);
            if group.iter().filter(|g| l(g) == mine).count() == 1 {
                return mine;
            }
        }
        c.join("_")
    };
    for it in sel {
        let parent = it.original.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let dir = dest_root.join(label(&parent));
        match place(&it, &dir) {
            Ok(o) => {
                let o = match o {
                    Outcome::Restored(p) | Outcome::Renamed(p) => Outcome::Elsewhere(p),
                    other => other,
                };
                s.add(&o);
                items.retain(|i| i.id != it.id);
            }
            Err(e) => s.failed.push(format!("{}: {e}", it.original.display())),
        }
    }
    let _ = write_index(&items);
    s
}

pub fn purge(id: &str) -> std::io::Result<()> {
    let _g = lock();
    let mut items = list();
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let it = items.remove(pos);
        let _ = fs::remove_file(&it.stored);
        write_index(&items)?;
    }
    Ok(())
}

pub fn empty() -> std::io::Result<usize> {
    let _g = lock();
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
        init_test_root();
        let d = std::env::temp_dir().join("ic_bin_test");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("a.png");
        fs::write(&f, b"hello").unwrap();
        move_in(&f).unwrap();
        assert!(!f.exists());
        let it = list().into_iter().find(|i| i.original == f).expect("항목 존재");
        fs::write(&f, b"new").unwrap(); // 같은 이름 충돌
        let r = match restore(&it.id).unwrap() {
            Outcome::Renamed(p) => p,
            o => panic!("{o:?}"),
        };
        assert_ne!(r, f);
        assert!(r.file_name().unwrap().to_string_lossy().contains("_복원"));
        assert_eq!(fs::read(&r).unwrap(), b"hello");
        assert!(list().iter().all(|i| i.original != f));
    }

    fn find(orig: &Path) -> BinItem {
        list().into_iter().find(|i| i.original == orig).expect("휴지통 항목")
    }

    #[test]
    fn identical_file_already_there_is_not_restored_twice() {
        init_test_root();
        let d = std::env::temp_dir().join("ic_restore_same");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("q.png");
        fs::write(&f, b"same").unwrap();
        move_in(&f).unwrap();
        fs::write(&f, b"same").unwrap(); // 내용이 같은 파일이 이미 있음
        let it = find(&f);
        assert!(matches!(restore(&it.id).unwrap(), Outcome::AlreadyThere(_)));
        assert!(!it.stored.exists() && list().iter().all(|i| i.original != f));
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
    }

    #[test]
    fn restore_to_disambiguates_same_folder_names() {
        init_test_root();
        let d = std::env::temp_dir().join("ic_restore_to");
        let _ = fs::remove_dir_all(&d);
        let (a, b) = (d.join("a").join("여행").join("1.png"), d.join("b").join("여행").join("2.png"));
        for f in [&a, &b] {
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, b"x").unwrap();
            move_in(f).unwrap();
        }
        let ids = vec![find(&a).id, find(&b).id];
        let dest = d.join("복원본");
        let s = restore_to(&ids, &dest);
        assert_eq!((s.elsewhere, s.failed.len()), (2, 0), "{s:?}");
        assert!(dest.join("a_여행").join("1.png").exists());
        assert!(dest.join("b_여행").join("2.png").exists());
    }

    #[test]
    fn unavailable_original_location_falls_back() {
        init_test_root();
        let bad = PathBuf::from(r"Q:\ic_nope\폴더\z.png"); // 존재하지 않는 드라이브
        let d = std::env::temp_dir().join("ic_restore_fb");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let tmp = d.join("z.png");
        fs::write(&tmp, b"zz").unwrap();
        move_in(&tmp).unwrap();
        {
            let _g = lock(); // 다른 테스트와 색인 갱신이 겹치지 않도록
            let mut items = list();
            for i in items.iter_mut().filter(|i| i.original == tmp) {
                i.original = bad.clone();
            }
            write_index(&items).unwrap();
        }
        match restore(&find(&bad).id).unwrap() {
            Outcome::Elsewhere(p) => {
                assert!(p.exists());
                let _ = fs::remove_file(p);
            }
            o => panic!("{o:?}"),
        }
    }
}
