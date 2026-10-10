//! App updates from GitHub Releases: find the latest release, download the
//! package for this computer, check it, and put it in place of the running app.
//!
//! Like git sync, this runs programs that are already installed instead of
//! bundling a network stack: `curl` (built into macOS, Windows 10+, and Fedora)
//! downloads, `ditto` (macOS) and `tar` (Windows) unpack.
//!
//! Each release has one package per system with a fixed name (see
//! [`package_name`]) and a `SHA256SUMS` file. A download whose checksum doesn't
//! match is never installed.
//!
//! Installing, per system:
//! - **macOS:** the new `KuNotes.app` replaces the running one (renaming a
//!   running app is allowed; it keeps running from the old files).
//! - **Windows:** the new `KuNotes.exe` replaces the running one, which is
//!   renamed to `KuNotes.exe.old` first (Windows can rename, but not delete, a
//!   running exe). The leftover is removed on the next launch.
//! - **Fedora:** the `.rpm` is installed with `pkexec dnf install`, which asks
//!   for the administrator password.
//!
//! Everything here blocks, so the app calls it on a background thread.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::error::{CoreError, Result};

/// The GitHub repository that publishes KuNotes releases.
pub const REPOSITORY: &str = "sofyan-rs/kunotes-gpui";

/// Checksums of every package in a release, as written by `sha256sum`.
pub const CHECKSUMS_NAME: &str = "SHA256SUMS";

/// Where the rpm installs KuNotes on Fedora.
const LINUX_INSTALL_PATH: &str = "/usr/bin/kunotes";

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The version without the leading `v`, e.g. `0.2.0`.
    pub version: String,
    /// The release's web page (notes and downloads).
    pub page_url: String,
    pub assets: Vec<Asset>,
}

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub download_url: String,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

/// The fields we read from GitHub's "latest release" JSON.
#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Reads GitHub's "latest release" JSON.
pub fn parse_release(json: &str) -> Result<Release> {
    let release: GitHubRelease = serde_json::from_str(json)
        .map_err(|error| CoreError::Update(format!("Unexpected answer from GitHub: {error}")))?;
    if release.draft || release.prerelease {
        return Err(CoreError::Update(
            "The latest release isn't a final one.".into(),
        ));
    }
    Ok(Release {
        version: release.tag_name.trim_start_matches('v').to_string(),
        page_url: release.html_url,
        assets: release
            .assets
            .into_iter()
            .map(|asset| Asset {
                name: asset.name,
                download_url: asset.browser_download_url,
            })
            .collect(),
    })
}

/// `major.minor.patch` from `1.2.3`, `v1.2.3`, or `1.2.3-beta`; `None` if it isn't a version.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    // Drop a pre-release or build suffix (`-beta.1`, `+abc`).
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// True if `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// The release package for a system (`std::env::consts::OS` and `ARCH`),
/// or `None` if releases don't include one for it.
pub fn package_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("KuNotes-macos-arm64.zip"),
        ("macos", "x86_64") => Some("KuNotes-macos-x64.zip"),
        ("windows", "x86_64") => Some("KuNotes-windows-x64.zip"),
        ("linux", "x86_64") => Some("KuNotes-fedora-x86_64.rpm"),
        _ => None,
    }
}

/// The package for the computer KuNotes is running on.
pub fn this_package_name() -> Option<&'static str> {
    package_name(std::env::consts::OS, std::env::consts::ARCH)
}

/// Finds `name`'s checksum (lowercase hex) in a `SHA256SUMS` file.
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        // `sha256sum` marks binary-mode files with `*`.
        let file = file.trim().trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Asks GitHub for the latest release. `None` if nothing has been released yet.
pub fn fetch_latest_release() -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    // `-w` prints the HTTP status on a last line of its own, so a 404 ("no
    // releases yet") can be told apart from a real failure.
    let output = run(curl()
        .args(["--max-time", "30", "-w", "\n%{http_code}"])
        .args(["-H", "Accept: application/vnd.github+json"])
        .arg(&url))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", text.as_ref()));
    match status.trim() {
        "200" => parse_release(body).map(Some),
        "404" => Ok(None),
        "403" | "429" => Err(CoreError::Update(
            "GitHub is limiting requests right now. Try again later.".into(),
        )),
        status => Err(CoreError::Update(format!(
            "GitHub answered with HTTP {status}."
        ))),
    }
}

/// Downloads `url` into `dest`.
pub fn download(url: &str, dest: &Path) -> Result<()> {
    run(curl()
        .args(["--fail", "--max-time", "900", "-o"])
        .arg(dest)
        .arg(url))?;
    Ok(())
}

/// Downloads this computer's package from `release` into `dir` and checks it
/// against the release's checksums. Returns the package's path.
pub fn download_package(release: &Release, dir: &Path) -> Result<PathBuf> {
    let name = this_package_name().ok_or_else(|| {
        CoreError::Update("Updates aren't published for this kind of computer.".into())
    })?;
    let missing = || CoreError::Update(format!("Release {} has no {name}.", release.version));
    let package = release.asset(name).ok_or_else(missing)?;
    let sums = release.asset(CHECKSUMS_NAME).ok_or_else(|| {
        CoreError::Update(format!(
            "Release {} has no {CHECKSUMS_NAME}.",
            release.version
        ))
    })?;

    fs::create_dir_all(dir)?;
    let sums_path = dir.join(CHECKSUMS_NAME);
    download(&sums.download_url, &sums_path)?;
    let expected = checksum_for(&fs::read_to_string(&sums_path)?, name).ok_or_else(missing)?;

    let package_path = dir.join(name);
    download(&package.download_url, &package_path)?;
    if sha256_of(&package_path)? != expected {
        fs::remove_file(&package_path)?;
        return Err(CoreError::Update(
            "The download was damaged (its checksum doesn't match). Nothing was changed.".into(),
        ));
    }
    Ok(package_path)
}

/// Downloads and installs `release` in place of the running app, using a
/// temporary folder that's removed afterwards. Returns the path to start the
/// new version from.
pub fn download_and_install(release: &Release) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("kunotes-update-{}", release.version));
    let result = download_package(release, &dir)
        .and_then(|package| install(&package, &std::env::current_exe()?));
    // A leftover download is harmless, so only an install failure is reported.
    let _cleanup = remove_if_exists(&dir);
    result
}

/// Installs a downloaded package in place of the running app (`current_exe`).
/// Returns the path to start the new version from.
pub fn install(package: &Path, current_exe: &Path) -> Result<PathBuf> {
    match std::env::consts::OS {
        "macos" => install_macos(package, current_exe),
        "windows" => install_windows(package, current_exe),
        "linux" => install_linux(package, current_exe),
        _ => Err(CoreError::Update(
            "Updates aren't supported on this system.".into(),
        )),
    }
}

fn install_macos(zip: &Path, current_exe: &Path) -> Result<PathBuf> {
    // The exe is at KuNotes.app/Contents/MacOS/kunotes.
    let bundle = current_exe
        .ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        .ok_or_else(|| {
            CoreError::Update("Only KuNotes.app can update itself (this is a dev build).".into())
        })?
        .to_path_buf();
    let folder = parent_of(&bundle)?;
    // Unpack next to the app, so the swap below is a rename on the same disk.
    let staging = folder.join(".KuNotes-update");
    remove_if_exists(&staging)?;
    run(Command::new("ditto")
        .arg("-x")
        .arg("-k")
        .arg(zip)
        .arg(&staging))?;
    let new_app = staging.join("KuNotes.app");
    if !new_app.join("Contents/MacOS/kunotes").is_file() {
        return Err(CoreError::Update(
            "The update package doesn't contain KuNotes.app.".into(),
        ));
    }
    let old = folder.join(".KuNotes-old.app");
    replace_with(&bundle, &new_app, &old)?;
    remove_if_exists(&old)?;
    remove_if_exists(&staging)?;
    Ok(bundle)
}

fn install_windows(zip: &Path, current_exe: &Path) -> Result<PathBuf> {
    let folder = parent_of(current_exe)?;
    let staging = folder.join(".kunotes-update");
    remove_if_exists(&staging)?;
    fs::create_dir_all(&staging)?;
    // `tar` ships with Windows 10 and later, and reads zip files.
    run(Command::new("tar")
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(&staging))?;
    let new_exe = staging.join("KuNotes.exe");
    if !new_exe.is_file() {
        return Err(CoreError::Update(
            "The update package doesn't contain KuNotes.exe.".into(),
        ));
    }
    let old = old_exe_path(current_exe);
    remove_if_exists(&old)?; // from an earlier update
    replace_with(current_exe, &new_exe, &old)?;
    remove_if_exists(&staging)?;
    // `old` is still running; it's removed on the next launch.
    Ok(current_exe.to_path_buf())
}

fn install_linux(rpm: &Path, current_exe: &Path) -> Result<PathBuf> {
    if current_exe != Path::new(LINUX_INSTALL_PATH) {
        return Err(CoreError::Update(
            "Only the KuNotes rpm package can update itself. Download the new version from the release page.".into(),
        ));
    }
    // pkexec shows the system's password prompt, then dnf upgrades the package.
    let output = Command::new("pkexec")
        .args(["dnf", "install", "-y"])
        .arg(rpm)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| CoreError::Update(format!("Couldn't run pkexec: {error}")))?;
    match output.status.code() {
        Some(0) => Ok(PathBuf::from(LINUX_INSTALL_PATH)),
        // pkexec: 126 = the password prompt was dismissed, 127 = not authorized.
        Some(126 | 127) => Err(CoreError::Update("The update was cancelled.".into())),
        _ => Err(CoreError::Update(format!(
            "dnf couldn't install the update: {}",
            last_line(&output.stderr)
        ))),
    }
}

/// Puts `new` at `target`, moving what was there to `old`. If the second
/// rename fails, the original is moved back so the app is never left missing.
pub fn replace_with(target: &Path, new: &Path, old: &Path) -> Result<()> {
    fs::rename(target, old)?;
    if let Err(error) = fs::rename(new, target) {
        return Err(match fs::rename(old, target) {
            Ok(()) => error.into(),
            Err(restore) => CoreError::Update(format!(
                "Couldn't install the update ({error}) or put the old version back ({restore}). It's at {}.",
                old.display()
            )),
        });
    }
    Ok(())
}

/// Where Windows keeps the replaced exe until the next launch: `KuNotes.exe.old`.
pub fn old_exe_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Removes what an earlier update left behind (Windows' old exe). Call at startup.
pub fn clean_up_after_update(current_exe: &Path) -> Result<()> {
    remove_if_exists(&old_exe_path(current_exe))
}

/// Removes an updater file or folder (never vault content).
fn remove_if_exists(path: &Path) -> Result<()> {
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn parent_of(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| CoreError::Update(format!("{} has no folder.", path.display())))
}

fn curl() -> Command {
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--location",
        "--user-agent",
        "KuNotes",
    ]);
    command
}

/// Runs a command; a failure's text is its last line of error output.
fn run(command: &mut Command) -> Result<Output> {
    command.stdin(Stdio::null());
    // A windowed app on Windows would flash a console window for each run.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .map_err(|error| CoreError::Update(format!("Couldn't run {program}: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Update(format!(
            "{program} failed: {}",
            last_line(&output.stderr)
        )));
    }
    Ok(output)
}

fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown error")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/sofyan-rs/kunotes-gpui/releases/tag/v0.2.0",
        "draft": false,
        "prerelease": false,
        "assets": [
            { "name": "KuNotes-macos-arm64.zip", "browser_download_url": "https://example.com/mac.zip", "size": 1 },
            { "name": "SHA256SUMS", "browser_download_url": "https://example.com/sums" }
        ]
    }"#;

    #[test]
    fn reads_github_release_json() {
        let release = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(release.version, "0.2.0");
        assert!(release.page_url.ends_with("/v0.2.0"));
        assert_eq!(
            release
                .asset("KuNotes-macos-arm64.zip")
                .unwrap()
                .download_url,
            "https://example.com/mac.zip"
        );
        assert!(release.asset("KuNotes-windows-x64.zip").is_none());
    }

    #[test]
    fn prereleases_are_not_offered() {
        let json = RELEASE_JSON.replace(r#""prerelease": false"#, r#""prerelease": true"#);
        assert!(parse_release(&json).is_err());
    }

    #[test]
    fn compares_versions_by_number() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.99.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
    }

    #[test]
    fn one_package_per_system() {
        assert_eq!(
            package_name("macos", "aarch64"),
            Some("KuNotes-macos-arm64.zip")
        );
        assert_eq!(
            package_name("windows", "x86_64"),
            Some("KuNotes-windows-x64.zip")
        );
        assert_eq!(
            package_name("linux", "x86_64"),
            Some("KuNotes-fedora-x86_64.rpm")
        );
        assert_eq!(package_name("linux", "aarch64"), None);
    }

    #[test]
    fn finds_checksum_lines() {
        let sums = "ABC123  KuNotes-macos-arm64.zip\n\
                    def456 *KuNotes-windows-x64.zip\n";
        assert_eq!(
            checksum_for(sums, "KuNotes-macos-arm64.zip").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            checksum_for(sums, "KuNotes-windows-x64.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(checksum_for(sums, "KuNotes-fedora-x86_64.rpm"), None);
    }

    #[test]
    fn old_exe_keeps_its_name() {
        assert_eq!(
            old_exe_path(Path::new("C:/Apps/KuNotes.exe")),
            Path::new("C:/Apps/KuNotes.exe.old")
        );
    }
}
