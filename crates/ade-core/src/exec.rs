//! Real subprocess + PATH lookup implementations — port of `src/exec.ts`.
//! argv arrays only; never a shell string (ISC-179). Never panics: spawn
//! failure and empty argv return `code: 127` like the oracle.

use crate::types::{ExecFn, ExecOpts, ExecResult, WhichFn};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

pub fn real_exec() -> ExecFn {
    Arc::new(|argv: &[String], opts: &ExecOpts| -> ExecResult {
        let Some(program) = argv.first() else {
            return ExecResult::failure(127, "empty argv");
        };
        let mut command = Command::new(program);
        command.args(&argv[1..]);
        // Children inherit the SAME resolved PATH the detector searches, or a
        // GUI launch would find `brew` and then fail to run it — and probes
        // that shell out (a harness asking its own plugin list, a CLI that
        // needs node) would report broken tools that are perfectly fine.
        if !is_shell_probe(argv) {
            command.env("PATH", resolved_path());
        }
        if let Some(cwd) = &opts.cwd {
            command.current_dir(cwd);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.stdin(if opts.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return ExecResult::failure(127, &error.to_string()),
        };
        if let (Some(stdin_text), Some(mut stdin)) = (&opts.stdin, child.stdin.take()) {
            let _ = stdin.write_all(stdin_text.as_bytes());
            drop(stdin);
        }
        match child.wait_with_output() {
            Ok(output) => ExecResult {
                code: output.status.code().unwrap_or(1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            Err(error) => ExecResult::failure(127, &error.to_string()),
        }
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// True for the one call that must NOT get the resolved PATH: the login-shell
/// probe that computes it. Overriding PATH there would make the shell report
/// back the value we just handed it, and the answer would be circular.
fn is_shell_probe(argv: &[String]) -> bool {
    argv.len() == 4 && argv[1] == "-l" && argv[2] == "-c" && argv[3].contains("$PATH")
}

/// The `PATH` this process should search, computed once.
///
/// Never the raw inherited value: a GUI app launched from Finder or launchd
/// gets `/usr/bin:/bin:/usr/sbin:/sbin`, on which no developer tool exists, and
/// a detector that trusts it reports a fully-equipped machine as empty. The
/// login shell is consulted once and cached, because spawning a shell per
/// `which` would be absurd for a probe that runs every four seconds.
fn resolved_path() -> String {
    static RESOLVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            let inherited = std::env::var("PATH").unwrap_or_default();
            let home = std::env::var("HOME").unwrap_or_default();
            let shell = std::env::var("SHELL").ok();
            let from_shell = crate::envpath::login_shell_path(&real_exec(), shell.as_deref());
            crate::envpath::effective_path(&inherited, from_shell.as_deref(), &home)
        })
        .clone()
}

pub fn real_which() -> WhichFn {
    Arc::new(|name: &str| -> Option<String> {
        if name.contains('/') {
            let path = Path::new(name);
            return is_executable(path).then(|| name.to_string());
        }
        let path_env = resolved_path();
        for dir in path_env.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = Path::new(dir).join(name);
            if is_executable(&candidate) {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::run_argv;

    #[test]
    fn empty_argv_is_127() {
        let exec = real_exec();
        let result = exec(&[], &ExecOpts::default());
        assert_eq!(result.code, 127);
        assert_eq!(result.stderr, "empty argv");
    }

    #[test]
    fn missing_binary_is_127_not_a_panic() {
        let exec = real_exec();
        let result = run_argv(&exec, &["definitely-not-a-real-binary-xyzzy"]);
        assert_eq!(result.code, 127);
        assert!(!result.stderr.is_empty());
    }

    #[test]
    fn captures_stdout_stderr_and_code() {
        let exec = real_exec();
        let ok = run_argv(&exec, &["printf", "out"]);
        assert_eq!(ok.code, 0);
        assert_eq!(ok.stdout, "out");
        // Nonzero exit + stderr without any shell involvement.
        let fail = run_argv(&exec, &["ls", "/definitely-not-a-real-dir-xyzzy"]);
        assert!(fail.code != 0);
        assert!(!fail.stderr.is_empty());
    }

    #[test]
    fn stdin_is_delivered() {
        let exec = real_exec();
        let owned: Vec<String> = vec!["cat".into()];
        let result = exec(
            &owned,
            &ExecOpts {
                cwd: None,
                stdin: Some("hello".into()),
            },
        );
        assert_eq!(result.code, 0);
        assert_eq!(result.stdout, "hello");
    }

    #[test]
    fn which_finds_real_binaries_and_rejects_missing() {
        let which = real_which();
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-binary-xyzzy").is_none());
    }
}
