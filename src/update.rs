//! Update checker: asks GitHub for the newest release, and on request downloads and installs it.
//!
//! - **AppImage** (Linux, Steam Deck): the new AppImage replaces the file that was launched (the
//!   `APPIMAGE` path), so launchers and Steam shortcuts keep working.
//! - **Windows zip**: the new files are unpacked next to the game; the running `.exe` and the old
//!   `assets` folder are renamed to `*.old` (Windows allows renaming a running program) and
//!   removed on the next start.
//! - **Anything else** (a development build): only says that a new version exists.
//!
//! Every download is checked against the release's `SHA256SUMS` file before it's used. All
//! network work runs on a background thread; the interface polls [`state`].

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use sha2::{Digest, Sha256};

const REPO: &str = "acdigitalmedia-hub/facet";
const SUMS: &str = "SHA256SUMS";
/// Refuse anything bigger than this (the builds are ~60 MB).
const MAX_DOWNLOAD: u64 = 600 << 20;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// The build for this system: (file name, url, size).
    file: Option<(String, String, u64)>,
    sums: Option<String>,
}

impl Release {
    /// Can this release be installed from inside the game?
    pub fn installable(&self) -> bool {
        self.file.is_some() && self.sums.is_some() && install_kind() != Install::Manual
    }
}

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// Couldn't reach GitHub (offline, rate limited, …).
    Offline,
    Available(Release),
    Downloading { version: String, got: u64, total: u64 },
    /// Installed; takes effect on restart.
    Ready(String),
    Failed(String),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);

pub fn state() -> State {
    STATE.lock().map(|s| s.clone()).unwrap_or_default()
}

fn set(s: State) {
    if let Ok(mut g) = STATE.lock() {
        *g = s;
    }
}

/// The running version. `FACET3D_FAKE_VERSION=0.0.1` pretends to be older, to test updating.
pub fn current() -> String {
    std::env::var("FACET3D_FAKE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Install {
    AppImage(PathBuf),
    /// The folder holding the exe and `assets`.
    Windows(PathBuf),
    Manual,
}

pub fn install_kind() -> Install {
    #[cfg(target_os = "linux")]
    if let Some(p) = std::env::var_os("APPIMAGE").map(PathBuf::from)
        && p.is_file()
    {
        return Install::AppImage(p);
    }
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && dir.join("assets").is_dir()
        && !exe.components().any(|c| c.as_os_str() == "target")
    {
        return Install::Windows(dir.to_path_buf());
    }
    Install::Manual
}

/// "v1.2.3" / "1.2.3-beta" → (1, 2, 3).
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let s = s.split(['-', '+']).next()?;
    let mut it = s.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(timeout).timeout_connect(Some(Duration::from_secs(10))).build().into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent
        .get(url)
        .header("User-Agent", concat!("facet-updater/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())
}

/// Starts a check in the background (does nothing while one is running or a download is going).
pub fn start_check() {
    {
        let Ok(mut g) = STATE.lock() else { return };
        if matches!(*g, State::Checking | State::Downloading { .. } | State::Ready(_)) {
            return;
        }
        *g = State::Checking;
    }
    std::thread::spawn(|| {
        set(match check() {
            Ok(Some(r)) => State::Available(r),
            Ok(None) => State::UpToDate,
            Err(e) => {
                eprintln!("update check: {e}");
                State::Offline
            }
        })
    });
}

fn check() -> Result<Option<Release>, String> {
    let a = agent(Some(Duration::from_secs(15)));
    // Test hook: FACET3D_UPDATE_API=url reads the release from somewhere else (a local server).
    let api = std::env::var("FACET3D_UPDATE_API").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let text = get(&a, &api)?.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("no tag in the release")?;
    let (Some(new), Some(cur)) = (parse_version(tag), parse_version(&current())) else {
        return Err(format!("can't compare versions {tag} and {}", current()));
    };
    if new <= cur {
        return Ok(None);
    }
    let want = match install_kind() {
        Install::Windows(_) => "-windows.zip".to_string(),
        _ => format!("-{}.AppImage", std::env::consts::ARCH),
    };
    let mut file = None;
    let mut sums = None;
    for a in v["assets"].as_array().into_iter().flatten() {
        let (Some(name), Some(url)) = (a["name"].as_str(), a["browser_download_url"].as_str()) else { continue };
        if name == SUMS {
            sums = Some(url.to_string());
        } else if name.ends_with(&want) {
            file = Some((name.to_string(), url.to_string(), a["size"].as_u64().unwrap_or(0)));
        }
    }
    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        notes: clean_notes(v["body"].as_str().unwrap_or("")),
        page: v["html_url"].as_str().unwrap_or("").to_string(),
        file,
        sums,
    }))
}

/// GitHub's markdown notes → plain lines.
fn clean_notes(s: &str) -> String {
    let mut out = Vec::new();
    for line in s.lines() {
        let l = line.trim().trim_start_matches('#').trim();
        let l = l.replace("**", "");
        let l = match l.strip_prefix("* ").or_else(|| l.strip_prefix("- ")) {
            Some(rest) => format!("• {rest}"),
            None => l,
        };
        if !(l.is_empty() && out.last().is_some_and(|p: &String| p.is_empty())) {
            out.push(l);
        }
    }
    out.join("\n").trim().to_string()
}

/// Downloads and installs a release in the background.
pub fn start_install(rel: Release) {
    set(State::Downloading { version: rel.version.clone(), got: 0, total: 0 });
    std::thread::spawn(move || {
        set(match install(&rel) {
            Ok(()) => State::Ready(rel.version.clone()),
            Err(e) => {
                eprintln!("update: {e}");
                State::Failed(e)
            }
        })
    });
}

fn install(rel: &Release) -> Result<(), String> {
    let (name, url, size) = rel.file.clone().ok_or("this release has no download for this system")?;
    let sums_url = rel.sums.clone().ok_or("this release has no checksums")?;
    let a = agent(None);
    let sums = get(&a, &sums_url)?.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let expected = sums
        .lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, f)| f.trim().trim_start_matches('*') == name)
        .map(|(h, _)| h.to_ascii_lowercase())
        .ok_or_else(|| format!("{name} isn't listed in {SUMS}"))?;

    match install_kind() {
        Install::AppImage(target) => {
            let dir = target.parent().ok_or("bad AppImage path")?;
            let tmp = dir.join(".facet-update.download");
            download(&a, &url, size, &tmp, &expected, &rel.version).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            // Replacing the file is safe while it runs: the old one stays mounted until exit.
            std::fs::rename(&tmp, &target).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                format!("couldn't replace {}: {e}", target.display())
            })
        }
        Install::Windows(dir) => {
            let tmp = dir.join(".facet-update.zip");
            download(&a, &url, size, &tmp, &expected, &rel.version).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
            let r = install_windows(&dir, &tmp);
            let _ = std::fs::remove_file(&tmp);
            r
        }
        Install::Manual => Err("this copy can't update itself: download the new version from GitHub".into()),
    }
}

/// Streams `url` into `path`, checking its SHA-256.
fn download(a: &ureq::Agent, url: &str, size: u64, path: &Path, expected: &str, version: &str) -> Result<(), String> {
    let mut res = get(a, url)?;
    let total = res.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(size);
    if total > MAX_DOWNLOAD {
        return Err("download is unexpectedly large".into());
    }
    let mut body = res.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut out = std::fs::File::create(path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| format!("couldn't write the download: {e}"))?;
        got += n as u64;
        set(State::Downloading { version: version.to_string(), got, total });
    }
    out.sync_all().map_err(|e| e.to_string())?;
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != expected {
        return Err("the download didn't match its checksum, so it wasn't installed".into());
    }
    Ok(())
}

#[cfg(windows)]
fn install_windows(dir: &Path, zip_path: &Path) -> Result<(), String> {
    let staging = dir.join(".facet-update");
    let _ = std::fs::remove_dir_all(&staging);
    let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { continue };
        // Drop the top folder ("Facet/…").
        let rel: PathBuf = rel.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = staging.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
    }
    let exe_name = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_owned())).unwrap_or_else(|| "Facet.exe".into());
    let mut moved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut result = Ok(());
    for item in std::fs::read_dir(&staging).map_err(|e| e.to_string())?.flatten() {
        let name = item.file_name();
        let dest = if name.eq_ignore_ascii_case("Facet.exe") { dir.join(&exe_name) } else { dir.join(&name) };
        let mut old = dest.clone().into_os_string();
        old.push(".old");
        let old = PathBuf::from(old);
        remove_any(&old);
        let backup = if dest.exists() {
            if let Err(e) = std::fs::rename(&dest, &old) {
                result = Err(format!("couldn't move {} aside: {e}", dest.display()));
                break;
            }
            Some(old)
        } else {
            None
        };
        if let Err(e) = std::fs::rename(item.path(), &dest) {
            if let Some(b) = &backup {
                let _ = std::fs::rename(b, &dest);
            }
            result = Err(format!("couldn't install {}: {e}", dest.display()));
            break;
        }
        moved.push((dest, backup));
    }
    if result.is_err() {
        // Put everything back the way it was.
        for (dest, backup) in moved.into_iter().rev() {
            remove_any(&dest);
            if let Some(b) = backup {
                let _ = std::fs::rename(b, &dest);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&staging);
    result
}

#[cfg(not(windows))]
fn install_windows(_dir: &Path, _zip: &Path) -> Result<(), String> {
    Err("not a Windows build".into())
}

#[allow(dead_code)]
fn remove_any(p: &Path) {
    if p.is_dir() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Removes what a previous update left behind (call at startup).
pub fn clean_up() {
    match install_kind() {
        Install::Windows(dir) => {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.ends_with(".old") || n.starts_with(".facet-update") {
                    remove_any(&e.path());
                }
            }
        }
        Install::AppImage(p) => {
            if let Some(d) = p.parent() {
                let _ = std::fs::remove_file(d.join(".facet-update.download"));
            }
        }
        Install::Manual => {}
    }
}

/// Starts the installed new version and quits this one.
pub fn restart() -> ! {
    let mut cmd = match install_kind() {
        Install::AppImage(p) => std::process::Command::new(p),
        _ => std::process::Command::new(std::env::current_exe().unwrap_or_else(|_| "Facet.exe".into())),
    };
    // The new AppImage sets these up itself.
    for k in ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "FACET_ASSETS", "FACET3D_FAKE_VERSION"] {
        cmd.env_remove(k);
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("restart: {e}");
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.2.10"), Some((0, 2, 10)));
        assert_eq!(parse_version("1.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("0.3.0-beta.1"), Some((0, 3, 0)));
        assert!(parse_version("v0.10.0") > parse_version("0.9.9"));
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn notes() {
        let n = clean_notes("## What's Changed\r\n* Faster **towers** by x\r\n\r\n\r\n**Full Changelog**: a...b");
        assert_eq!(n, "What's Changed\n• Faster towers by x\n\nFull Changelog: a...b");
    }
}
