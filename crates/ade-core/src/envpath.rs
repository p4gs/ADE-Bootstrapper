//! Working out where the user's tools actually live.
//!
//! A process launched from Finder, the Dock or launchd inherits a minimal
//! `PATH` — typically `/usr/bin:/bin:/usr/sbin:/sbin`. Nothing installed by
//! Homebrew, cargo, npm or an installer script is on it. A detector that trusts
//! that `PATH` reports a developer machine with seventeen tools on it as
//! completely empty, which is exactly what the Control Center did: every row
//! "not installed", every capability a gap, a screen full of alarm that was
//! entirely false.
//!
//! The tray never had this bug because its launchd plist hardcodes a PATH. That
//! fixed one launch surface and left the other broken, which is why this lives
//! in the core instead: the answer should not depend on how the app was started.

use crate::types::{ExecFn, ExecOpts};

/// Where developer tools are conventionally installed, in the order a shell
/// would normally search them. Used to backfill a minimal inherited `PATH`.
pub fn fallback_dirs(home: &str) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for suffix in [
        ".local/bin",
        ".cargo/bin",
        ".npm-global/bin",
        ".bun/bin",
        "go/bin",
    ] {
        if !home.is_empty() {
            dirs.push(format!("{home}/{suffix}"));
        }
    }
    for absolute in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        dirs.push(absolute.to_string());
    }
    dirs
}

/// Merge the sources of truth about `PATH`, best first, preserving order and
/// dropping duplicates and empties.
///
/// The login shell wins because it is what the owner would get in a terminal —
/// the definition of "installed" a person actually means. The inherited value
/// comes next so an explicitly-set PATH is honoured. Conventional locations
/// backfill last, so detection degrades to something useful rather than to
/// nothing when no shell can be consulted.
pub fn effective_path(inherited: &str, shell: Option<&str>, home: &str) -> String {
    let mut merged: Vec<String> = Vec::new();
    for source in [shell.unwrap_or(""), inherited] {
        for dir in source.split(':') {
            add_dir(&mut merged, dir);
        }
    }
    for dir in fallback_dirs(home) {
        add_dir(&mut merged, &dir);
    }
    merged.join(":")
}

fn add_dir(merged: &mut Vec<String>, dir: &str) {
    let dir = dir.trim();
    if dir.is_empty() || merged.iter().any(|seen| seen == dir) {
        return;
    }
    merged.push(dir.to_string());
}

/// Ask the owner's login shell what `PATH` it has.
///
/// The command is a constant — no interpolation of anything the environment
/// supplies — and runs as an argv array, never through a shell string. A login
/// shell is used because that is what sources the profile that puts Homebrew
/// and friends on the path in the first place.
pub fn login_shell_path(exec: &ExecFn, shell: Option<&str>) -> Option<String> {
    let shell = shell?;
    // Only ever invoke an absolute path, so a relative or empty SHELL cannot
    // resolve against the current directory.
    if !shell.starts_with('/') {
        return None;
    }
    let argv = vec![
        shell.to_string(),
        "-l".to_string(),
        "-c".to_string(),
        "printf %s \"$PATH\"".to_string(),
    ];
    let result = exec(&argv, &ExecOpts::default());
    if result.code != 0 {
        return None;
    }
    let value = result.stdout.trim().to_string();
    (!value.is_empty() && value.contains('/')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fake_exec;

    #[test]
    fn a_minimal_launch_environment_still_finds_developer_tools() {
        // Exactly what Finder hands a .app — and what made every capability
        // read "not installed" on a machine that had them all.
        let launchd = "/usr/bin:/bin:/usr/sbin:/sbin";
        let path = effective_path(launchd, None, "/Users/x");
        for expected in [
            "/opt/homebrew/bin",
            "/Users/x/.local/bin",
            "/Users/x/.cargo/bin",
            "/usr/local/bin",
        ] {
            assert!(path.contains(expected), "{expected} missing from {path}");
        }
    }

    #[test]
    fn the_login_shell_leads_and_nothing_is_listed_twice() {
        let path = effective_path(
            "/usr/bin:/bin",
            Some("/opt/homebrew/bin:/usr/bin"),
            "/Users/x",
        );
        let dirs: Vec<&str> = path.split(':').collect();
        assert_eq!(dirs[0], "/opt/homebrew/bin", "shell PATH leads");
        // /usr/bin appears in the shell PATH, the inherited PATH and the
        // fallbacks; it must appear once.
        assert_eq!(dirs.iter().filter(|d| **d == "/usr/bin").count(), 1);
        assert!(dirs.iter().all(|d| !d.is_empty()));
    }

    #[test]
    fn empty_and_ragged_inputs_degrade_to_the_conventional_locations() {
        let path = effective_path("", None, "/Users/x");
        assert!(path.contains("/opt/homebrew/bin"));
        // Empty segments and stray whitespace never become path entries.
        let ragged = effective_path("::/usr/bin: :", Some(""), "/Users/x");
        assert!(ragged.split(':').all(|dir| !dir.trim().is_empty()));
        // No home means no home-relative guesses, but absolutes still apply.
        let homeless = effective_path("", None, "");
        assert!(homeless.contains("/opt/homebrew/bin"));
        assert!(!homeless.contains("//"));
    }

    #[test]
    fn the_login_shell_is_consulted_safely_or_not_at_all() {
        let ok = fake_exec(&[(
            "/bin/zsh -l -c printf %s \"$PATH\"",
            (0, "/opt/homebrew/bin:/usr/bin", ""),
        )]);
        assert_eq!(
            login_shell_path(&ok, Some("/bin/zsh")).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        // A relative SHELL is never executed — it would resolve against the
        // current directory.
        assert!(login_shell_path(&ok, Some("zsh")).is_none());
        assert!(login_shell_path(&ok, None).is_none());
        // A shell that fails, or answers with something that is not a path,
        // yields nothing rather than poisoning PATH.
        let broken = fake_exec(&[("/bin/zsh -l -c printf %s \"$PATH\"", (1, "", "boom"))]);
        assert!(login_shell_path(&broken, Some("/bin/zsh")).is_none());
        let noise = fake_exec(&[("/bin/zsh -l -c printf %s \"$PATH\"", (0, "not-a-path", ""))]);
        assert!(login_shell_path(&noise, Some("/bin/zsh")).is_none());
    }
}

#[cfg(test)]
mod exec_integration {
    /// The detector and the actions it runs must search the same places.
    /// Finding `brew` and then failing to execute it is worse than not finding
    /// it: the row offers a button that cannot work.
    #[test]
    fn a_child_process_inherits_the_resolved_path_not_the_launch_path() {
        let exec = crate::exec::real_exec();
        let result = exec(
            &[
                "/bin/sh".to_string(),
                "-c".into(),
                "printf %s \"$PATH\"".into(),
            ],
            &crate::types::ExecOpts::default(),
        );
        assert_eq!(result.code, 0);
        let child_path = result.stdout;
        for expected in ["/opt/homebrew/bin", "/usr/bin"] {
            assert!(
                child_path.contains(expected),
                "child PATH missing {expected}: {child_path}"
            );
        }
    }
}
