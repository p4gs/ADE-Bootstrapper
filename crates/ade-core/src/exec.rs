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

pub fn real_which() -> WhichFn {
    Arc::new(|name: &str| -> Option<String> {
        if name.contains('/') {
            let path = Path::new(name);
            return is_executable(path).then(|| name.to_string());
        }
        let path_env = std::env::var("PATH").unwrap_or_default();
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
