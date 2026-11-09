//! updater.ts: checks GitHub for the latest Billable release, downloads its
//! disk image, and installs it in place through a detached helper script that
//! waits for Billable to quit, swaps the app bundle and relaunches it.
//!
//! The network work runs on a background thread with a small blocking HTTP
//! client; the public functions hand back GPUI tasks. Download progress is
//! delivered on the main thread.

use std::{
    cmp::Ordering,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use gpui::{App, Global, Task};
use serde::{Deserialize, Serialize};

const REPO_OWNER: &str = "bulledh-beep";
const REPO_NAME: &str = "Billable";
/// The status is cached so views can ask freely.
const CACHE_FOR: Duration = Duration::from_secs(5 * 60);

/// `UpdateStatus` (shared/types.ts). Numbers are JS numbers, so `f64`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub release_notes: Option<String>,
    pub download_url: Option<String>,
    pub download_size_bytes: Option<f64>,
    pub published_at: Option<String>,
    /// Always set (an ISO timestamp); optional to match the Settings page.
    pub last_checked_at: Option<String>,
}

/// `UpdateProgress` (the `updater:progress` event).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateProgress {
    pub received: f64,
    pub total: f64,
    pub percent: f64,
}

/// The release notes for one version (What's New).
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    pub name: String,
    pub body: String,
    pub html_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

/// `app.getVersion()`.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------------------------------------------------------------------------
// Versions and assets
// ---------------------------------------------------------------------------

/// `parseInt(p, 10) || 0`
fn parse_part(p: &str) -> f64 {
    let s = p.trim_start_matches(billable_core::js::is_js_whitespace);
    let (neg, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return 0.0;
    }
    let v: f64 = digits.parse().unwrap_or(0.0);
    if neg {
        -v
    } else {
        v
    }
}

/// A version's three numbers and its pre-release tag ("" for a release).
fn parse_version(v: &str) -> ([f64; 3], &str) {
    // .replace(/^v/i, '').split('-', 2): JS's limit drops anything after a
    // second dash ("1.2.3-beta-2" has the tag "beta").
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    let mut halves = v.split('-');
    let main = halves.next().unwrap_or("");
    let pre = halves.next().unwrap_or("");
    let mut parts = [0.0; 3];
    for (slot, p) in parts.iter_mut().zip(main.split('.')) {
        *slot = parse_part(p);
    }
    (parts, pre)
}

/// Compare two semver-ish versions ("1.2.3" or "1.2.3-beta.1"). Pre-releases
/// sort below the matching release (1.2.3-beta < 1.2.3); two tags compare as
/// strings.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (pa, a_pre) = parse_version(a);
    let (pb, b_pre) = parse_version(b);
    for i in 0..3 {
        if pa[i] != pb[i] {
            return if pa[i] > pb[i] { Ordering::Greater } else { Ordering::Less };
        }
    }
    match (a_pre.is_empty(), b_pre.is_empty()) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => a_pre.cmp(b_pre),
    }
}

/// `/arm64.*\.dmg$/i`
fn is_arm64_dmg(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match (lower.find("arm64"), lower.strip_suffix(".dmg")) {
        (Some(at), Some(stem)) => at + "arm64".len() <= stem.len(),
        _ => false,
    }
}

/// The arm64 disk image, else any disk image.
pub fn pick_dmg_asset(release: &GitHubRelease) -> Option<&GitHubAsset> {
    release
        .assets
        .iter()
        .find(|a| is_arm64_dmg(&a.name))
        .or_else(|| release.assets.iter().find(|a| a.name.to_lowercase().ends_with(".dmg")))
}

/// The status a release gives this build.
pub fn status_for(release: Option<&GitHubRelease>, current: &str, checked_at: String) -> UpdateStatus {
    let mut status = UpdateStatus {
        current_version: current.to_string(),
        latest_version: None,
        update_available: false,
        release_url: None,
        release_notes: None,
        download_url: None,
        download_size_bytes: None,
        published_at: None,
        last_checked_at: Some(checked_at),
    };
    // Drafts and pre-releases are skipped.
    if let Some(release) = release.filter(|r| !r.draft && !r.prerelease) {
        let latest = release.tag_name.strip_prefix(['v', 'V']).unwrap_or(&release.tag_name).to_string();
        status.update_available = compare_versions(&latest, current) == Ordering::Greater;
        status.latest_version = Some(latest);
        status.release_url = Some(release.html_url.clone());
        status.release_notes = release.body.clone();
        status.published_at = release.published_at.clone();
        if let Some(asset) = pick_dmg_asset(release) {
            status.download_url = Some(asset.browser_download_url.clone());
            status.download_size_bytes = Some(asset.size);
        }
    }
    status
}

/// `new Date().toISOString()`
fn iso_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(format!("Billable/{}", current_version()))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .into()
}

fn api_get(url: &str) -> Result<(u16, String)> {
    let mut res = agent().get(url).header("Accept", "application/vnd.github+json").call()?;
    let status = res.status().as_u16();
    let body = res.body_mut().read_to_string()?;
    Ok((status, body))
}

/// GitHub's `releases/latest`.
pub fn fetch_latest_release() -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let (status, body) = api_get(&url)?;
    if status == 404 {
        bail!("No releases published yet");
    }
    if status != 200 {
        bail!("GitHub API {status}: {}", billable_core::js::js_slice(&body, 200));
    }
    serde_json::from_str(&body).map_err(|e| anyhow!("Bad JSON from GitHub: {e}"))
}

/// The release notes for a version tag; `None` when there's no such release.
pub fn fetch_release_notes(version: &str) -> Result<Option<ReleaseInfo>> {
    let tag = if version.starts_with('v') { version.to_string() } else { format!("v{version}") };
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/tags/{tag}");
    let (status, body) = api_get(&url)?;
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        bail!("GitHub API {status}");
    }
    let json: GitHubRelease = serde_json::from_str(&body)?;
    Ok(Some(ReleaseInfo {
        name: json.name.filter(|n| !n.is_empty()).unwrap_or(tag),
        body: json.body.unwrap_or_default(),
        html_url: json.html_url,
    }))
}

/// `path.basename(new URL(url).pathname) || 'Billable-update.dmg'`
pub fn file_name_for(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("");
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if name.is_empty() {
        "Billable-update.dmg".into()
    } else {
        name.into()
    }
}

/// Download `url` to `dest`, reporting progress whenever the percentage
/// moves (only when the server sends a length).
pub fn download_to(url: &str, dest: &Path, mut progress: impl FnMut(UpdateProgress)) -> Result<()> {
    let mut res = agent().get(url).call()?;
    let status = res.status().as_u16();
    if status != 200 {
        bail!("Download failed: HTTP {status}");
    }
    let total = res
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .map(parse_part)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0);
    let mut file = File::create(dest)?;
    let mut reader = res.body_mut().as_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0f64;
    let mut last_percent = -1f64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        received += n as f64;
        if total > 0.0 {
            let percent = billable_core::js::js_round(received / total * 100.0);
            if percent != last_percent {
                last_percent = percent;
                progress(UpdateProgress { received, total, percent });
            }
        }
    }
    file.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// The `.app` folder holding an executable (…/Billable.app/Contents/MacOS/billable).
pub fn bundle_containing(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|p| p.as_os_str().to_string_lossy().ends_with(".app")).map(Path::to_path_buf)
}

/// The running app bundle, or `None` in a dev build (Electron's
/// `app.isPackaged`), which shouldn't try to replace itself.
pub fn app_bundle_path() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return None;
    }
    bundle_containing(&std::env::current_exe().ok()?)
}

/// Whether "Install and relaunch" works (`updater.canInstall()`).
pub fn can_install() -> bool {
    app_bundle_path().is_some()
}

/// Written to a temp file and run detached as Billable quits. It waits for
/// Billable to exit, mounts the disk image, replaces the app bundle in place,
/// ejects, and relaunches.
///
/// Args: $1 the downloaded disk image, $2 the Billable.app to replace, $3 the
/// running Billable's process id (the native executable is `billable`, so the
/// Electron helper's `pgrep -x Billable` wouldn't find it).
pub const HELPER_SCRIPT: &str = r#"#!/bin/bash
set +e

DMG="$1"
APP="$2"
PID="$3"
LOG="${TMPDIR:-/tmp}/billable-update.log"

echo "[$(date)] starting install helper" > "$LOG"
echo "DMG=$DMG" >> "$LOG"
echo "APP=$APP" >> "$LOG"

# Wait for Billable to exit (max ~15s)
for i in $(seq 1 50); do
  kill -0 "$PID" 2>/dev/null || break
  sleep 0.3
done

# If still running, force-quit
if kill -0 "$PID" 2>/dev/null; then
  echo "force-quitting still-running Billable" >> "$LOG"
  kill "$PID" 2>/dev/null
  sleep 1
fi

# Mount the DMG
MOUNT_OUT=$(hdiutil attach "$DMG" -nobrowse -noverify -noautoopen 2>&1)
echo "$MOUNT_OUT" >> "$LOG"
MOUNT=$(echo "$MOUNT_OUT" | grep -Eo "/Volumes/[^[:space:]]+" | tail -n1)

if [ -z "$MOUNT" ] || [ ! -d "$MOUNT" ]; then
  echo "mount failed" >> "$LOG"
  open "$APP"
  exit 1
fi

# Replace the .app bundle
if ! rm -rf "$APP"; then
  echo "could not remove old app" >> "$LOG"
  hdiutil detach "$MOUNT" -quiet 2>/dev/null
  open "$APP" 2>/dev/null
  exit 1
fi

if ! cp -R "$MOUNT/Billable.app" "$APP"; then
  echo "copy failed" >> "$LOG"
  hdiutil detach "$MOUNT" -quiet 2>/dev/null
  exit 1
fi

# Strip quarantine so Gatekeeper doesn't whine on relaunch
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

# Re-register with Launch Services so Spotlight/icon caches update
LSREG=/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister
if [ -x "$LSREG" ]; then
  "$LSREG" -f "$APP" >/dev/null 2>&1 || true
fi

# Eject DMG and clean up
hdiutil detach "$MOUNT" -quiet 2>/dev/null || true
rm -f "$DMG"

# Relaunch
open "$APP"
echo "[$(date)] install complete, relaunched" >> "$LOG"

# Self-cleanup
rm -f "$0"
"#;

/// Write the helper into `dir` and start it in its own session so it
/// outlives Billable.
pub fn spawn_helper(dir: &Path, dmg: &Path, app: &Path, pid: u32) -> Result<PathBuf> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let helper = dir.join(format!("billable-update-{millis}.sh"));
    std::fs::write(&helper, HELPER_SCRIPT)?;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755))?;
    let mut cmd = std::process::Command::new("/bin/bash");
    cmd.arg(&helper)
        .arg(dmg)
        .arg(app)
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Node's `detached: true`: a new session, away from Billable's.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(helper)
}

/// `fs.accessSync(dir, W_OK)`
fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

// ---------------------------------------------------------------------------
// The app-facing API (ipc `updater:*`)
// ---------------------------------------------------------------------------

static LAST: Mutex<Option<(UpdateStatus, Instant)>> = Mutex::new(None);

/// The last status (`updater.cached()`).
pub fn cached() -> Option<UpdateStatus> {
    LAST.lock().ok().and_then(|l| l.as_ref().map(|(s, _)| s.clone()))
}

/// `updater.check(force)`: GitHub's latest release, cached for five minutes
/// unless `force`. Errors aren't cached.
pub fn check(force: bool, cx: &App) -> Task<Result<UpdateStatus>> {
    if !force {
        if let Ok(last) = LAST.lock() {
            if let Some((status, at)) = last.as_ref() {
                if at.elapsed() < CACHE_FOR {
                    return Task::ready(Ok(status.clone()));
                }
            }
        }
    }
    cx.background_executor().spawn(async move {
        let checked_at = iso_now();
        let release = fetch_latest_release()?;
        let status = status_for(Some(&release), current_version(), checked_at);
        if let Ok(mut last) = LAST.lock() {
            *last = Some((status.clone(), Instant::now()));
        }
        Ok(status)
    })
}

/// `updater.releaseNotes(version)`
pub fn release_notes(version: &str, cx: &App) -> Task<Result<Option<ReleaseInfo>>> {
    let version = version.to_string();
    cx.background_executor().spawn(async move { fetch_release_notes(&version) })
}

/// Run blocking work on its own thread, handing progress to `on_progress` on
/// the main thread as it comes in.
fn with_progress<T: Send + 'static>(
    work: impl FnOnce(&dyn Fn(UpdateProgress)) -> Result<T> + Send + 'static,
    on_progress: impl Fn(UpdateProgress, &mut App) + 'static,
    cx: &App,
) -> Task<Result<T>> {
    enum Msg<T> {
        Progress(UpdateProgress),
        Done(Result<T>),
    }
    let (tx, rx) = mpsc::channel::<Msg<T>>();
    let spawned = std::thread::Builder::new().name("billable-update".into()).spawn(move || {
        let progress_tx = tx.clone();
        let result = work(&move |p| {
            let _ = progress_tx.send(Msg::Progress(p));
        });
        let _ = tx.send(Msg::Done(result));
    });
    if let Err(e) = spawned {
        return Task::ready(Err(e.into()));
    }
    cx.spawn(async move |cx| loop {
        let mut latest = None;
        loop {
            match rx.try_recv() {
                Ok(Msg::Progress(p)) => latest = Some(p),
                Ok(Msg::Done(result)) => {
                    if let Some(p) = latest {
                        cx.update(|cx| on_progress(p, cx));
                    }
                    return result;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Err(anyhow!("The download stopped unexpectedly")),
            }
        }
        if let Some(p) = latest {
            cx.update(|cx| on_progress(p, cx));
        }
        cx.background_executor().timer(Duration::from_millis(50)).await;
    })
}

/// `updater.download(url)`: download the disk image to ~/Downloads and open
/// it in Finder.
pub fn download(url: String, on_progress: impl Fn(UpdateProgress, &mut App) + 'static, cx: &App) -> Task<Result<PathBuf>> {
    let job = with_progress(
        move |progress| {
            let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| anyhow!("No home folder"))?;
            let dest = home.join("Downloads").join(file_name_for(&url));
            download_to(&url, &dest, progress)?;
            Ok(dest)
        },
        on_progress,
        cx,
    );
    cx.spawn(async move |cx| {
        let path = job.await?;
        cx.update(|cx| cx.open_with_system(&path));
        Ok(path)
    })
}

/// `updater.install(url)`: download the disk image quietly to the temp
/// folder, start the helper, and quit half a second later so it can swap the
/// bundle and relaunch.
pub fn install(url: String, on_progress: impl Fn(UpdateProgress, &mut App) + 'static, cx: &App) -> Task<Result<()>> {
    let Some(bundle) = app_bundle_path() else {
        return Task::ready(Err(anyhow!("Could not locate the Billable.app bundle. Use Download instead.")));
    };
    if !bundle.exists() {
        return Task::ready(Err(anyhow!("App bundle not found at {}", bundle.display())));
    }
    // /Applications is writable by the user on personal Macs.
    let parent = bundle.parent().map(Path::to_path_buf).unwrap_or_default();
    if !writable(&parent) {
        return Task::ready(Err(anyhow!(
            "Cannot write to {} — install location requires admin access. Use Download instead.",
            parent.display()
        )));
    }
    let pid = std::process::id();
    let job = with_progress(
        move |progress| {
            let tmp = std::env::temp_dir();
            let dmg = tmp.join(file_name_for(&url));
            download_to(&url, &dmg, progress)?;
            spawn_helper(&tmp, &dmg, &bundle, pid)?;
            Ok(())
        },
        on_progress,
        cx,
    );
    cx.spawn(async move |cx| {
        job.await?;
        // A beat for the "installing, quitting" state to show.
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// "Check for Updates…"
// ---------------------------------------------------------------------------

/// A menu-driven check, as each view watching sees it.
pub enum MenuCheck<'a> {
    Started,
    Finished(&'a Result<UpdateStatus, String>),
}

type Watcher = Rc<dyn Fn(MenuCheck<'_>, &mut App)>;

#[derive(Default)]
struct MenuWatchers(Vec<(&'static str, Watcher)>);
impl Global for MenuWatchers {}

/// Follow checks started from the app menu (useUpdater's
/// `menu:check-updates` listener). A view registering again under the same
/// key replaces its earlier registration.
pub fn watch_menu_checks(key: &'static str, f: impl Fn(MenuCheck<'_>, &mut App) + 'static, cx: &mut App) {
    let watchers = &mut cx.default_global::<MenuWatchers>().0;
    watchers.retain(|(k, _)| *k != key);
    watchers.push((key, Rc::new(f)));
}

/// "Check for Updates…" in the app menu: one forced check, shared with
/// every watching view.
pub fn check_from_menu(cx: &mut App) {
    let watchers: Vec<Watcher> = cx.try_global::<MenuWatchers>().map(|w| w.0.iter().map(|(_, f)| f.clone()).collect()).unwrap_or_default();
    for w in &watchers {
        w(MenuCheck::Started, cx);
    }
    let task = check(true, cx);
    cx.spawn(async move |cx| {
        let result = task.await.map_err(|e| e.to_string());
        cx.update(|cx| {
            for w in &watchers {
                w(MenuCheck::Finished(&result), cx);
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset { name: name.into(), browser_download_url: format!("https://example.com/{name}"), size: 100.0 }
    }

    fn release(tag: &str, assets: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.into(),
            name: Some(format!("Billable {tag}")),
            body: Some("Notes".into()),
            html_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/tag/{tag}"),
            published_at: Some("2026-10-21T17:15:49Z".into()),
            prerelease: false,
            draft: false,
            assets: assets.iter().map(|n| asset(n)).collect(),
        }
    }

    #[test]
    fn compares_versions_like_updater_ts() {
        use Ordering::*;
        assert_eq!(compare_versions("1.5.3", "1.5.3"), Equal);
        assert_eq!(compare_versions("v1.5.3", "1.5.3"), Equal);
        assert_eq!(compare_versions("V2.0.0", "v1.9.9"), Greater);
        assert_eq!(compare_versions("1.5.10", "1.5.9"), Greater);
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Greater);
        assert_eq!(compare_versions("1.5.3", "2.0.0"), Less);
        // Missing parts are zeros; extra parts are ignored.
        assert_eq!(compare_versions("2", "2.0.0"), Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Less);
        assert_eq!(compare_versions("1.2.3.9", "1.2.3"), Equal);
        // parseInt: leading digits only, junk is zero.
        assert_eq!(compare_versions("1.2.3rc", "1.2.3"), Equal);
        assert_eq!(compare_versions("1.x.0", "1.0.0"), Equal);
        // Pre-releases sort below their release, and compare as strings.
        assert_eq!(compare_versions("1.2.3-beta", "1.2.3"), Less);
        assert_eq!(compare_versions("1.2.3", "1.2.3-beta"), Greater);
        assert_eq!(compare_versions("1.2.3-beta.2", "1.2.3-beta.1"), Greater);
        assert_eq!(compare_versions("1.2.3-alpha", "1.2.3-beta"), Less);
        assert_eq!(compare_versions("1.2.3-beta.10", "1.2.3-beta.2"), Less);
        assert_eq!(compare_versions("1.2.4-beta", "1.2.3"), Greater);
        // split('-', 2) keeps only the first tag piece.
        assert_eq!(compare_versions("1.2.3-beta-2", "1.2.3-beta"), Equal);
    }

    #[test]
    fn picks_the_arm64_disk_image() {
        let r = release("v1.5.3", &["Billable-1.5.3.dmg", "Billable-1.5.3-arm64.dmg", "latest-mac.yml"]);
        assert_eq!(pick_dmg_asset(&r).unwrap().name, "Billable-1.5.3-arm64.dmg");
        let r = release("v1.5.3", &["Billable-1.5.3-ARM64.DMG"]);
        assert_eq!(pick_dmg_asset(&r).unwrap().name, "Billable-1.5.3-ARM64.DMG");
        // Falls back to any disk image.
        let r = release("v1.5.3", &["Billable-1.5.3-arm64.zip", "Billable-1.5.3-x64.dmg"]);
        assert_eq!(pick_dmg_asset(&r).unwrap().name, "Billable-1.5.3-x64.dmg");
        let r = release("v1.5.3", &["Billable-1.5.3-arm64.dmg.blockmap", "Billable.DMG"]);
        assert_eq!(pick_dmg_asset(&r).unwrap().name, "Billable.DMG");
        // "arm64" has to come before the extension.
        assert!(!is_arm64_dmg("arm64.dmg.zip"));
        assert!(is_arm64_dmg("arm64.dmg"));
        assert!(!is_arm64_dmg("Billable.dmgarm64"));
        let r = release("v1.5.3", &["Billable-1.5.3-arm64.zip", "notes.txt"]);
        assert!(pick_dmg_asset(&r).is_none());
    }

    #[test]
    fn status_from_a_release() {
        let r = release("v2.1.0", &["Billable-2.1.0-arm64.dmg"]);
        let s = status_for(Some(&r), "2.0.0", "2026-11-08T00:00:00.000Z".into());
        assert!(s.update_available);
        assert_eq!(s.latest_version.as_deref(), Some("2.1.0"));
        assert_eq!(s.download_url.as_deref(), Some("https://example.com/Billable-2.1.0-arm64.dmg"));
        assert_eq!(s.download_size_bytes, Some(100.0));
        assert_eq!(s.release_notes.as_deref(), Some("Notes"));

        let s = status_for(Some(&release("v1.5.3", &[])), "2.0.0", "t".into());
        assert!(!s.update_available);
        assert_eq!(s.latest_version.as_deref(), Some("1.5.3"));
        assert!(s.download_url.is_none());

        let mut pre = release("v3.0.0-beta", &["a-arm64.dmg"]);
        pre.prerelease = true;
        let s = status_for(Some(&pre), "2.0.0", "t".into());
        assert!(!s.update_available);
        assert!(s.latest_version.is_none());
        assert_eq!(s.current_version, "2.0.0");
    }

    #[test]
    fn parses_githubs_release_json() {
        let json = r#"{"tag_name":"v1.5.3","name":"Billable v1.5.3","body":null,"html_url":"https://github.com/x","published_at":"2026-10-21T17:15:49Z","prerelease":false,"draft":false,
            "assets":[{"name":"Billable-1.5.3-arm64.dmg","browser_download_url":"https://github.com/d.dmg","size":113488898,"id":1}],"id":5}"#;
        let r: GitHubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(r.assets[0].size, 113488898.0);
        assert!(r.body.is_none());
    }

    #[test]
    fn download_file_names() {
        assert_eq!(file_name_for("https://github.com/bulledh-beep/Billable/releases/download/v1.5.3/Billable-1.5.3-arm64.dmg"), "Billable-1.5.3-arm64.dmg");
        assert_eq!(file_name_for("https://example.com/a/b.dmg?x=1#y"), "b.dmg");
        assert_eq!(file_name_for("https://example.com/a/b/"), "b");
        assert_eq!(file_name_for("https://example.com/"), "Billable-update.dmg");
        assert_eq!(file_name_for("https://example.com"), "Billable-update.dmg");
    }

    /// Live: GitHub's latest release and a tag's notes.
    /// `cargo test -p billable live_github -- --ignored`
    #[test]
    #[ignore]
    fn live_github() {
        let release = fetch_latest_release().unwrap();
        let status = status_for(Some(&release), current_version(), iso_now());
        assert!(status.latest_version.is_some());
        assert!(status.download_url.is_some());
        assert!(fetch_release_notes("1.5.3").unwrap().is_some());
        assert!(fetch_release_notes("0.0.0-nope").unwrap().is_none());
    }

    /// Downloads a disk image from `BILLABLE_UPDATER_TEST_URL` into
    /// `BILLABLE_UPDATER_TEST_DIR` and runs the helper against the fake
    /// bundle `$DIR/old/Billable.app` (waits on this test's process id).
    #[test]
    #[ignore]
    fn download_and_install_fake_bundle() {
        let dir = PathBuf::from(std::env::var("BILLABLE_UPDATER_TEST_DIR").unwrap());
        let url = std::env::var("BILLABLE_UPDATER_TEST_URL").unwrap();
        let dmg = dir.join("tmp").join(file_name_for(&url));
        let mut seen = vec![];
        download_to(&url, &dmg, |p| seen.push(p.percent)).unwrap();
        assert_eq!(seen.last(), Some(&100.0));
        assert!(seen.windows(2).all(|w| w[0] < w[1]));
        let helper = spawn_helper(&dir.join("tmp"), &dmg, &dir.join("old/Billable.app"), std::process::id()).unwrap();
        assert!(helper.exists());
        assert!(!dir.join("tmp/missing-404.dmg").exists());
        let missing = download_to(&format!("{url}.missing"), &dir.join("tmp/missing-404.dmg"), |_| {});
        assert_eq!(missing.unwrap_err().to_string(), "Download failed: HTTP 404");
    }

    #[test]
    fn finds_the_app_bundle() {
        assert_eq!(bundle_containing(Path::new("/Applications/Billable.app/Contents/MacOS/billable")), Some(PathBuf::from("/Applications/Billable.app")));
        assert_eq!(bundle_containing(Path::new("/Users/me/native/target/debug/billable")), None);
        // The nearest bundle wins.
        assert_eq!(bundle_containing(Path::new("/x/Outer.app/Contents/Inner.app/Contents/MacOS/b")), Some(PathBuf::from("/x/Outer.app/Contents/Inner.app")));
    }
}
