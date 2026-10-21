//! Software update: checks the latest GitHub Release, downloads the package for this platform
//! and replaces the installed application in place.
//!
//! All network and file work runs on a background thread; the GUI polls [`Updater::state`].

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GITHUB_REPO: &str = "is90057/SketchCAD";
const USER_AGENT: &str = concat!("SketchCAD/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches(['v', 'V'])
    }

    /// The downloadable package for this platform, if the release has one.
    pub fn platform_asset(&self) -> Option<&ReleaseAsset> {
        let find = |needle: &str| {
            self.assets
                .iter()
                .find(|a| a.name.contains(needle) && a.name.ends_with(".zip"))
        };
        if cfg!(target_os = "windows") {
            find("windows-x64")
        } else if cfg!(target_os = "macos") {
            find("macos-universal").or_else(|| {
                find(if cfg!(target_arch = "aarch64") { "macos-arm64" } else { "macos-x86_64" })
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Idle,
    Checking { manual: bool },
    /// Only reported for manual checks
    UpToDate,
    Available(ReleaseInfo),
    Downloading { version: String, received: u64, total: u64 },
    /// Installed; the app must restart to run the new version
    Installed { version: String, relaunch: PathBuf },
    /// `release_url` lets the user fall back to a manual download
    Failed { message: String, release_url: Option<String> },
}

/// Handle shared between the GUI and the background worker.
#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<UpdateState>>,
}

impl Default for Updater {
    fn default() -> Self {
        Self { state: Arc::new(Mutex::new(UpdateState::Idle)) }
    }
}

impl Updater {
    pub fn state(&self) -> UpdateState {
        self.state.lock().map(|s| s.clone()).unwrap_or(UpdateState::Idle)
    }

    fn set(&self, state: UpdateState, ctx: &egui::Context) {
        if let Ok(mut s) = self.state.lock() {
            *s = state;
        }
        ctx.request_repaint();
    }

    pub fn dismiss(&self) {
        if let Ok(mut s) = self.state.lock() {
            *s = UpdateState::Idle;
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state(), UpdateState::Checking { .. } | UpdateState::Downloading { .. })
    }

    /// Checks GitHub for a newer release. Automatic (startup) checks stay silent unless an
    /// update is found; manual checks also report "up to date" and errors.
    pub fn check(&self, ctx: &egui::Context, manual: bool) {
        if self.is_busy() {
            return;
        }
        self.set(UpdateState::Checking { manual }, ctx);
        let (this, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let next = match fetch_latest_release() {
                Ok(Some(release)) if is_newer(release.version(), env!("CARGO_PKG_VERSION")) => {
                    UpdateState::Available(release)
                }
                Ok(_) if manual => UpdateState::UpToDate,
                Err(e) if manual => UpdateState::Failed { message: format!("無法檢查更新：{e:#}"), release_url: None },
                _ => UpdateState::Idle,
            };
            this.set(next, &ctx);
        });
    }

    /// Whether `release` can be installed in place by this build (else: open the release page).
    pub fn can_self_update(release: &ReleaseInfo) -> bool {
        release.platform_asset().is_some() && install_target().is_ok()
    }

    /// Downloads and installs `release`. Falls back to the release page when this build
    /// cannot update itself (development builds, unsupported platforms).
    pub fn install(&self, ctx: &egui::Context, release: ReleaseInfo) {
        let Some(asset) = release.platform_asset().cloned() else {
            ctx.open_url(egui::OpenUrl::new_tab(&release.html_url));
            self.dismiss();
            return;
        };
        let target = match install_target() {
            Ok(target) => target,
            Err(_) => {
                ctx.open_url(egui::OpenUrl::new_tab(&release.html_url));
                self.dismiss();
                return;
            }
        };

        let version = release.version().to_string();
        self.set(UpdateState::Downloading { version: version.clone(), received: 0, total: asset.size }, ctx);
        let (this, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = download_and_install(&asset, &target, |received, total| {
                this.set(UpdateState::Downloading { version: version.clone(), received, total }, &ctx);
            });
            let next = match result {
                Ok(relaunch) => UpdateState::Installed { version, relaunch },
                Err(e) => UpdateState::Failed {
                    message: format!("更新失敗：{e:#}"),
                    release_url: Some(release.html_url.clone()),
                },
            };
            this.set(next, &ctx);
        });
    }
}

/// Launches the freshly installed version; the caller then closes this instance.
pub fn relaunch(path: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-n").arg(path).spawn()?;
    } else {
        std::process::Command::new(path).spawn()?;
    }
    Ok(())
}

/// Removes the previous executable left behind by a Windows update (best effort).
pub fn cleanup_previous_install() {
    if cfg!(target_os = "windows") {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::remove_file(exe.with_extension("exe.old"));
        }
    }
}

/// `true` when `candidate` is a higher semantic version than `current` ("0.10.0" > "0.9.1").
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parse(candidate), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build()
}

/// Latest published (non-draft, non-prerelease) release, or `None` when there is none yet.
fn fetch_latest_release() -> Result<Option<ReleaseInfo>> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(resp) => {
            let body = resp.into_string()?;
            Ok(Some(serde_json::from_str(&body).context("GitHub 回應格式錯誤")?))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(ureq::Error::Status(code, _)) => bail!("GitHub 回應 HTTP {code}"),
        Err(e) => Err(anyhow!(e)),
    }
}

/// Where this installation lives and therefore what gets replaced.
enum InstallTarget {
    /// Windows: the running `SketchCAD.exe`
    WindowsExe(PathBuf),
    /// macOS: the `SketchCAD.app` bundle containing the running binary
    MacBundle(PathBuf),
}

fn install_target() -> Result<InstallTarget> {
    if cfg!(debug_assertions) {
        bail!("development build");
    }
    let exe = std::env::current_exe()?;
    if cfg!(target_os = "windows") {
        return Ok(InstallTarget::WindowsExe(exe));
    }
    if cfg!(target_os = "macos") {
        if let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
            return Ok(InstallTarget::MacBundle(bundle.to_path_buf()));
        }
        bail!("not running from an .app bundle");
    }
    bail!("unsupported platform")
}

fn download_and_install(
    asset: &ReleaseAsset,
    target: &InstallTarget,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf> {
    // Stage next to the installation so the final swap is a same-volume rename
    let install_dir = match target {
        InstallTarget::WindowsExe(exe) => exe.parent(),
        InstallTarget::MacBundle(bundle) => bundle.parent(),
    }
    .context("無法判斷安裝位置")?;
    let staging = install_dir.join(".sketchcad-update");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).with_context(|| format!("無法寫入安裝資料夾 {}", install_dir.display()))?;

    let zip_path = staging.join(&asset.name);
    download(&asset.browser_download_url, &zip_path, progress)?;

    let result = match target {
        InstallTarget::WindowsExe(exe) => install_windows(&zip_path, exe),
        InstallTarget::MacBundle(bundle) => install_macos(&zip_path, &staging, bundle),
    };
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn download(url: &str, dest: &Path, progress: impl Fn(u64, u64)) -> Result<()> {
    let resp = agent().get(url).call().context("下載失敗")?;
    let total = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    let mut last_report = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        received += n as u64;
        if received - last_report >= 256 * 1024 {
            progress(received, total);
            last_report = received;
        }
    }
    progress(received, total);
    if total > 0 && received != total {
        bail!("下載不完整 ({received} / {total} bytes)");
    }
    Ok(())
}

/// Swaps in the new `SketchCAD.exe`: a running exe cannot be overwritten on Windows, but it
/// can be renamed, so the old one becomes `SketchCAD.exe.old` (deleted on next start).
fn install_windows(zip_path: &Path, exe: &Path) -> Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .map(|f| f.name().to_ascii_lowercase().ends_with("sketchcad.exe"))
                .unwrap_or(false)
        })
        .context("更新封包中找不到 SketchCAD.exe")?;

    let new_exe = exe.with_extension("exe.new");
    {
        let mut entry = archive.by_index(index)?;
        let mut out = std::fs::File::create(&new_exe)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    let old_exe = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old_exe);
    std::fs::rename(exe, &old_exe).context("無法替換執行檔（可能需要系統管理員權限）")?;
    if let Err(e) = std::fs::rename(&new_exe, exe) {
        let _ = std::fs::rename(&old_exe, exe); // roll back
        return Err(e).context("無法安裝新版執行檔");
    }
    Ok(exe.to_path_buf())
}

/// Replaces the whole `.app` bundle. The running process keeps its already-open files, so the
/// old bundle can be moved away while the app is still running.
fn install_macos(zip_path: &Path, staging: &Path, bundle: &Path) -> Result<PathBuf> {
    let extract_dir = staging.join("extracted");
    let status = std::process::Command::new("ditto")
        .arg("-x")
        .arg("-k")
        .arg(zip_path)
        .arg(&extract_dir)
        .status()?;
    if !status.success() {
        bail!("解壓縮更新封包失敗");
    }
    let new_app = std::fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .context("更新封包中找不到 .app")?;

    let old_app = staging.join("previous.app");
    std::fs::rename(bundle, &old_app).context("無法替換應用程式（請確認有寫入權限）")?;
    if let Err(e) = std::fs::rename(&new_app, bundle) {
        let _ = std::fs::rename(&old_app, bundle); // roll back
        return Err(e).context("無法安裝新版應用程式");
    }
    Ok(bundle.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.5.0", "0.4.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.0"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.4", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(!is_newer("0.4.0-beta", "0.4.0"));
    }

    #[test]
    fn picks_platform_asset_and_strips_tag_prefix() {
        let asset = |name: &str| ReleaseAsset {
            name: name.into(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: 1,
        };
        let release = ReleaseInfo {
            tag_name: "v0.5.0".into(),
            body: None,
            html_url: String::new(),
            assets: vec![
                asset("SketchCAD-v0.5.0-windows-x64.zip"),
                asset("SketchCAD-v0.5.0-macos-universal.dmg"),
                asset("SketchCAD-v0.5.0-macos-universal.zip"),
            ],
        };
        assert_eq!(release.version(), "0.5.0");
        let picked = release.platform_asset().map(|a| a.name.as_str());
        if cfg!(target_os = "macos") {
            assert_eq!(picked, Some("SketchCAD-v0.5.0-macos-universal.zip"));
        } else if cfg!(target_os = "windows") {
            assert_eq!(picked, Some("SketchCAD-v0.5.0-windows-x64.zip"));
        }
    }
}
