// SPDX-License-Identifier: GPL-3.0-or-later
//! GitHub Releases 기반 자동 업데이트.
//! 최신 릴리스의 설치 파일(*Setup*.exe)을 내려받아 SHA-256 을 검증한 뒤 조용히 설치한다.

use eframe::egui::Context;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const REPO: &str = "designloves2/image-cleaner";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub sha256: Option<String>,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release, f32),
    Ready(Release, PathBuf),
    Failed(String),
}

pub type Shared = Arc<Mutex<UpdateState>>;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build())
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into()
}

fn parse_ver(v: &str) -> Vec<u64> {
    v.trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (mut a, mut b) = (parse_ver(remote), parse_ver(local));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

fn fetch_latest(current: &str) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent()
        .get(&url)
        .header("User-Agent", "ImageCleaner-Updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let txt = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() || !is_newer(&tag, current) {
        return Ok(None);
    }
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let asset = assets
        .iter()
        .find(|a| {
            let n = a["name"].as_str().unwrap_or("").to_lowercase();
            n.ends_with(".exe") && n.contains("setup")
        })
        .ok_or("릴리스에 설치 파일이 없습니다")?;
    let name = asset["name"].as_str().unwrap_or("").to_string();
    let mut sha = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|s| s.to_lowercase());
    if sha.is_none() {
        // 대체: <설치파일>.sha256 자산
        if let Some(s) = assets.iter().find(|a| a["name"].as_str() == Some(&format!("{name}.sha256"))) {
            if let Some(u) = s["browser_download_url"].as_str() {
                if let Ok(mut r) = agent().get(u).header("User-Agent", "ImageCleaner-Updater").call() {
                    if let Ok(t) = r.body_mut().read_to_string() {
                        sha = t.split_whitespace().next().map(|s| s.to_lowercase());
                    }
                }
            }
        }
    }
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        notes: v["body"].as_str().unwrap_or("").to_string(),
        url: asset["browser_download_url"].as_str().ok_or("다운로드 주소가 없습니다")?.to_string(),
        sha256: sha,
        size: asset["size"].as_u64().unwrap_or(0),
    }))
}

fn set(shared: &Shared, ctx: &Context, s: UpdateState) {
    *shared.lock().unwrap() = s;
    ctx.request_repaint();
}

/// 백그라운드에서 새 버전 확인. auto_download 이면 발견 즉시 내려받는다.
pub fn check(shared: Shared, ctx: Context, auto_download: bool) {
    set(&shared, &ctx, UpdateState::Checking);
    std::thread::spawn(move || match fetch_latest(CURRENT) {
        Ok(Some(rel)) => {
            set(&shared, &ctx, UpdateState::Available(rel.clone()));
            if auto_download {
                download(shared, ctx, rel);
            }
        }
        Ok(None) => set(&shared, &ctx, UpdateState::UpToDate),
        Err(e) => set(&shared, &ctx, UpdateState::Failed(e)),
    });
}

pub fn download(shared: Shared, ctx: Context, rel: Release) {
    std::thread::spawn(move || {
        let res = (|| -> Result<PathBuf, String> {
            let want = rel.sha256.clone().ok_or("체크섬을 확인할 수 없어 업데이트를 중단했습니다")?;
            let dir = std::env::temp_dir().join("ImageCleaner-Update");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(format!("ImageCleaner-Setup-{}.exe", rel.version));
            let resp = agent().get(&rel.url).header("User-Agent", "ImageCleaner-Updater").call().map_err(|e| e.to_string())?;
            let total = resp.body().content_length().unwrap_or(rel.size).max(1);
            let mut reader = resp.into_body().into_reader();
            let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 64 * 1024];
            let (mut done, mut last) = (0u64, 0.0f32);
            loop {
                let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                let frac = done as f32 / total as f32;
                if frac - last > 0.01 {
                    last = frac;
                    set(&shared, &ctx, UpdateState::Downloading(rel.clone(), frac.min(1.0)));
                }
            }
            drop(f);
            let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
            if got != want {
                let _ = std::fs::remove_file(&path);
                return Err("다운로드한 파일의 체크섬이 일치하지 않습니다".into());
            }
            Ok(path)
        })();
        match res {
            Ok(p) => set(&shared, &ctx, UpdateState::Ready(rel, p)),
            Err(e) => set(&shared, &ctx, UpdateState::Failed(e)),
        }
    });
}

/// 설치 프로그램을 조용히 실행하고 앱을 종료한다(설치 후 자동 재실행).
pub fn install_and_exit(path: &std::path::Path) {
    let _ = std::process::Command::new(path).args(["/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/NORESTART"]).spawn();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("v1.2.0", "1.1.9"));
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("2.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v0.9.9", "1.0.0"));
    }

    /// 실제 GitHub 릴리스를 조회하는 네트워크 테스트: cargo test -- --ignored
    #[test]
    #[ignore]
    fn finds_published_release() {
        let r = fetch_latest("0.0.1").expect("조회 성공").expect("새 버전 발견");
        assert!(r.url.ends_with(".exe") && r.sha256.as_deref().map(|h| h.len()) == Some(64), "{r:?}");
        assert!(fetch_latest("999.0.0").unwrap().is_none());
    }
}
