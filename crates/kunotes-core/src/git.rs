//! Git sync: keeps a vault in step with a git remote (e.g. a GitHub repo) by
//! running the `git` program installed on the computer.
//!
//! Using the installed `git` means KuNotes never sees a password: signing in
//! is whatever git already uses (macOS Keychain, Git Credential Manager,
//! `gh auth login`, SSH keys). If git would need to ask for a password, it
//! fails instead of waiting, and the error says so.
//!
//! One sync is: commit local changes, fetch, merge the remote, push. When the
//! same file changed on both sides, nothing is lost: this computer's version
//! stays in place and the other one is saved next to it as
//! `Note (conflict 2026-10-09).md`.
//!
//! Everything here blocks while git runs, so the app calls it on a background thread.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{CoreError, Result};
use crate::fs_ops::{atomic_write, unique_path};

/// Written to `.gitignore` when connecting a vault that has none.
const GITIGNORE: &str = "# Written by KuNotes\n.DS_Store\nThumbs.db\n.*.kunotes.tmp\n";

/// What a sync did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Files the merge changed on this computer (to reload open notes).
    pub changed: Vec<PathBuf>,
    /// Copies saved because a file changed on both sides.
    pub conflicts: Vec<PathBuf>,
}

/// True if a `git` program can be run.
pub fn is_installed() -> bool {
    git_command(Path::new("."))
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The remote the vault syncs with, if it has one.
pub fn remote_url(root: &Path) -> Option<String> {
    let output = run(root, &["remote", "get-url", "origin"]).ok()?;
    let url = output.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// Starts syncing `root` with `url`: makes the vault a git repository if it
/// isn't one, sets the remote, and runs a first sync (which brings in the
/// remote's notes and merges them with the ones already here).
pub fn connect(root: &Path, url: &str) -> Result<SyncReport> {
    let url = url.trim();
    if url.is_empty() {
        return Err(CoreError::Git(
            "Enter the address of a git repository.".into(),
        ));
    }
    if !root.join(".git").exists() {
        run(root, &["init"])?;
        // A new repository starts on `main`, whatever git's default is.
        run(root, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
    }
    if !root.join(".gitignore").exists() {
        atomic_write(&root.join(".gitignore"), GITIGNORE.as_bytes())?;
    }
    if remote_url(root).is_some() {
        run(root, &["remote", "set-url", "origin", url])?;
    } else {
        run(root, &["remote", "add", "origin", url])?;
    }
    sync(root)
}

/// Stops syncing: removes the remote. The history in `.git` stays.
pub fn disconnect(root: &Path) -> Result<()> {
    run(root, &["remote", "remove", "origin"])?;
    Ok(())
}

/// One sync: commit, fetch, merge, push.
pub fn sync(root: &Path) -> Result<SyncReport> {
    commit_changes(root)?;
    let mut report = SyncReport::default();
    let Some(branch) = remote_branch(root)? else {
        // An empty remote: just send what's here.
        if has_commits(root) {
            run(root, &["push", "-u", "origin", "HEAD:refs/heads/main"])?;
        }
        return Ok(report);
    };

    run(root, &["fetch", "origin", &branch])?;
    let theirs = format!("refs/remotes/origin/{branch}");
    let before = head(root);
    let up_to_date = match &before {
        Some(_) => run(root, &["merge-base", "--is-ancestor", &theirs, "HEAD"]).is_ok(),
        None => false,
    };
    if !up_to_date {
        let mut args = commit_settings(root);
        args.extend([
            "merge",
            "--no-edit",
            "--no-verify",
            "--allow-unrelated-histories",
            "-m",
            "Merge notes from the remote (KuNotes)",
            &theirs,
        ]);
        let merged = run(root, &args);
        if merged.is_err() {
            report.conflicts = resolve_conflicts(root)?;
            commit(root, "Merge notes from the remote (KuNotes)")?;
        }
        report.changed = changed_files(root, before.as_deref())?;
    }
    // Send our commits, if the remote doesn't have them yet.
    if run(root, &["merge-base", "--is-ancestor", "HEAD", &theirs]).is_err() {
        run(
            root,
            &["push", "origin", &format!("HEAD:refs/heads/{branch}")],
        )?;
    }
    Ok(report)
}

/// Commits local changes and tries to push them, giving up after `timeout`
/// (used when the app quits, so a slow network can't keep it open).
pub fn commit_and_push_quickly(root: &Path, timeout: Duration) -> Result<()> {
    commit_changes(root)?;
    let Some(branch) = remote_branch(root)? else {
        return Ok(());
    };
    let mut child = git_command(root)
        .args(["push", "origin", &format!("HEAD:refs/heads/{branch}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    while child.try_wait()?.is_none() {
        if started.elapsed() > timeout {
            // Best effort: the commit is safe locally and goes out next time.
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// Where the copy of a conflicting file goes: next to it, named with today's date.
/// `Plan.md` becomes `Plan (conflict 2026-10-09).md`.
pub fn conflict_copy_path(path: &Path, date: &str) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(""));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A locked note keeps both extensions: `Keys (conflict …).md.age`.
    let (stem, extension) = if crate::lock::is_locked_note(path) {
        let stem = name[..name.len() - ".md.age".len()].to_string();
        (stem, Some("md.age".to_string()))
    } else {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned());
        (stem, extension)
    };
    unique_path(
        dir,
        &format!("{stem} (conflict {date})"),
        extension.as_deref(),
    )
}

/// Today's date as `YYYY-MM-DD` (UTC).
pub fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    date_from_days((seconds / 86_400) as i64)
}

/// `YYYY-MM-DD` for a number of days since 1970-01-01.
fn date_from_days(days: i64) -> String {
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

// ----- Steps -----

/// Commits everything that changed, if anything did.
fn commit_changes(root: &Path) -> Result<()> {
    run(root, &["add", "-A"])?;
    if run(root, &["status", "--porcelain"])?.trim().is_empty() {
        return Ok(());
    }
    commit(root, "Update notes (KuNotes)")
}

fn commit(root: &Path, message: &str) -> Result<()> {
    let mut args = commit_settings(root);
    args.extend(["commit", "--no-verify", "-m", message]);
    run(root, &args)?;
    Ok(())
}

/// Settings for anything that makes a commit (commit, merge): never ask for a
/// signing key, and use a stand-in name and email if git has none set.
fn commit_settings(root: &Path) -> Vec<&'static str> {
    let mut args = vec!["-c", "commit.gpgsign=false"];
    let has_identity =
        run(root, &["config", "user.email"]).is_ok_and(|email| !email.trim().is_empty());
    if !has_identity {
        args.extend([
            "-c",
            "user.name=KuNotes",
            "-c",
            "user.email=kunotes@localhost",
        ]);
    }
    args
}

/// The remote's default branch, or `None` if the remote is empty.
fn remote_branch(root: &Path) -> Result<Option<String>> {
    let output = run(root, &["ls-remote", "--symref", "origin", "HEAD"])?;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("ref: refs/heads/")
            && let Some(branch) = rest.split_whitespace().next()
        {
            return Ok(Some(branch.to_string()));
        }
    }
    // Some servers don't report HEAD; fall back to a branch that exists.
    let branches = run(root, &["ls-remote", "--heads", "origin"])?;
    let names: Vec<&str> = branches
        .lines()
        .filter_map(|line| line.split("refs/heads/").nth(1))
        .collect();
    Ok(["main", "master"]
        .into_iter()
        .find(|name| names.contains(name))
        .or(names.first().copied())
        .map(str::to_string))
}

fn has_commits(root: &Path) -> bool {
    head(root).is_some()
}

fn head(root: &Path) -> Option<String> {
    run(root, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .ok()
        .map(|sha| sha.trim().to_string())
}

/// Files that differ between `before` and now (all tracked files if there was no `before`).
fn changed_files(root: &Path, before: Option<&str>) -> Result<Vec<PathBuf>> {
    let output = match before {
        Some(before) => run(root, &["diff", "--name-only", "-z", before, "HEAD"])?,
        None => run(root, &["ls-files", "-z"])?,
    };
    Ok(output
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(|name| root.join(name))
        .collect())
}

/// Settles a merge that stopped on conflicts. For each file:
/// - changed on both sides: ours stays, theirs is saved as a conflict copy;
/// - deleted here, changed there: their version comes back;
/// - changed here, deleted there: ours stays.
///
/// Returns the conflict copies.
fn resolve_conflicts(root: &Path) -> Result<Vec<PathBuf>> {
    let names = run(root, &["diff", "--name-only", "-z", "--diff-filter=U"])?;
    let date = today();
    let mut copies = Vec::new();
    for name in names.split('\0').filter(|name| !name.is_empty()) {
        let stages = run(root, &["ls-files", "-u", "--", name])?;
        let has = |stage: &str| {
            stages
                .lines()
                .any(|line| line.contains(&format!(" {stage}\t")))
        };
        let (ours, theirs) = (has("2"), has("3"));
        if ours && theirs {
            let their_bytes = run_bytes(root, &["show", &format!(":3:{name}")])?;
            run(root, &["checkout", "--ours", "--", name])?;
            let copy = conflict_copy_path(&root.join(name), &date);
            atomic_write(&copy, &their_bytes)?;
            let relative = copy
                .strip_prefix(root)
                .unwrap_or(&copy)
                .to_string_lossy()
                .into_owned();
            run(root, &["add", "--", name, &relative])?;
            copies.push(copy);
        } else if theirs {
            run(root, &["checkout", "--theirs", "--", name])?;
            run(root, &["add", "--", name])?;
        } else {
            run(root, &["add", "--", name])?;
        }
    }
    Ok(copies)
}

// ----- Running git -----

/// `git` set up to never stop and ask for anything.
fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0") // fail instead of asking for a password
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C") // English messages, so errors can be recognised
        .args(["-c", "core.quotepath=false"])
        // Sync files byte for byte: no `\n` ⇄ `\r\n` rewriting (common on
        // Windows), since KuNotes keeps each file's own line endings.
        .args(["-c", "core.autocrlf=false"])
        .stdin(Stdio::null());
    // Signing in to GitHub over HTTPS: let git borrow the GitHub CLI's login,
    // like `gh auth setup-git` would, without changing the user's git config.
    // (gh users with the SSH protocol don't have this set up.)
    if let Some(gh) = github_cli() {
        // The helper is run by a shell, so quote the path and use `/`.
        let gh = gh.to_string_lossy().replace('\\', "/");
        command.args([
            "-c".to_string(),
            format!("credential.https://github.com.helper=!\"{gh}\" auth git-credential"),
        ]);
    }
    // A windowed app on Windows would flash a console window for each git run.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The GitHub CLI (`gh`), if it's installed. Looked up once.
///
/// Apps started from the Finder or a desktop launcher don't get the terminal's
/// `PATH`, so the usual install folders are searched too.
fn github_cli() -> Option<&'static Path> {
    static GH: OnceLock<Option<PathBuf>> = OnceLock::new();
    GH.get_or_init(|| find_program("gh")).as_deref()
}

fn find_program(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/home/linuxbrew/.linuxbrew/bin",
        ]
        .map(PathBuf::from),
    );
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(program_files).join("GitHub CLI"));
    }
    dirs.into_iter()
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

/// Runs git and returns what it printed, or an error with its message.
fn run(root: &Path, args: &[&str]) -> Result<String> {
    let output = run_output(root, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Like `run`, but returns the raw bytes (file contents can be binary).
fn run_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    Ok(run_output(root, args)?.stdout)
}

fn run_output(root: &Path, args: &[&str]) -> Result<Output> {
    let child = git_command(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                CoreError::Git("Git isn't installed. Install it, then try again.".into())
            }
            _ => CoreError::Io(error),
        })?;
    // Reads both outputs at once, so a full pipe can't block git.
    let output = child.wait_with_output()?;
    if output.status.success() {
        return Ok(output);
    }
    Err(CoreError::Git(friendly_error(&String::from_utf8_lossy(
        &output.stderr,
    ))))
}

/// Turns git's error output into something a person can act on.
fn friendly_error(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("could not read username")
        || lower.contains("authentication failed")
        || lower.contains("terminal prompts disabled")
        || lower.contains("permission denied")
    {
        return "Git couldn't sign in to the remote. For GitHub, sign in with the GitHub CLI \
                (`gh auth login`), or use the SSH address (git@github.com:you/notes.git) \
                if you have an SSH key. Then sync again."
            .into();
    }
    if lower.contains("host key verification failed") {
        return "SSH doesn't know this server yet. Run `ssh -T git@github.com` once in a \
                terminal (answer \"yes\"), then sync again."
            .into();
    }
    if lower.contains("repository not found")
        || lower.contains("does not appear to be a git repository")
    {
        return "The remote repository wasn't found. Check the address.".into();
    }
    if lower.contains("could not resolve host") || lower.contains("unable to access") {
        return "Couldn't reach the remote. Check your internet connection.".into();
    }
    let message = stderr.trim().lines().last().unwrap_or("git failed").trim();
    message
        .trim_start_matches("fatal: ")
        .trim_start_matches("error: ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_days() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(20_370), "2025-10-09");
        assert_eq!(date_from_days(19_782), "2024-02-29"); // leap day
    }

    #[test]
    fn conflict_copies_sit_next_to_the_file() {
        let path = Path::new("vault").join("Projects").join("Plan.md");
        assert_eq!(
            conflict_copy_path(&path, "2026-10-09"),
            Path::new("vault")
                .join("Projects")
                .join("Plan (conflict 2026-10-09).md")
        );
        assert_eq!(
            conflict_copy_path(Path::new("Keys.md.age"), "2026-10-09"),
            Path::new("Keys (conflict 2026-10-09).md.age")
        );
    }

    #[test]
    fn sign_in_errors_are_explained() {
        let message = friendly_error(
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
        );
        assert!(message.contains("sign in"), "{message}");
        assert_eq!(friendly_error("error: something odd\n"), "something odd");
    }
}
