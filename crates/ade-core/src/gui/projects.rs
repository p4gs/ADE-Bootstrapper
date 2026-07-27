//! Project-scoped operations the Control Center offers (ISC-219).
//!
//! These live in core rather than in the GUI crate on purpose: the UI crates
//! are render loops, deliberately excluded from the coverage gate, so any
//! decision that can be got WRONG has to sit here where it is tested. The app
//! calls this and draws the result.

use crate::config::load_config;
use crate::gui::state::{load_gui_state, save_gui_state};
use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::registry::module_ids;
use crate::remove::remove_pipeline;
use crate::run::{make_ctx, PipelineDeps};
use std::path::Path;

fn harness_ids() -> Vec<&'static str> {
    HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

/// Withdraw ADE from `dir`, then stop tracking it in `home`'s gui state.
///
/// Unregistering afterwards is not tidiness: a repo with no `ade.json` left in
/// the project list would keep offering module toggles and verifies that can
/// only fail, which reads as the app being broken rather than the repo being
/// clean.
pub fn remove_ade_from_project(
    home: &Path,
    dir: &str,
    deps: &PipelineDeps,
) -> Result<String, String> {
    let dir_path = Path::new(dir);
    let config =
        load_config(dir_path, &module_ids(), &harness_ids()).map_err(|error| error.to_string())?;
    let ctx = make_ctx(dir_path, config, deps);
    let report = remove_pipeline(&ctx, true);

    let mut load = load_gui_state(home);
    load.state.projects.retain(|entry| entry != dir);
    save_gui_state(home, &load.state).map_err(|error| error.to_string())?;

    Ok(format!(
        "ade removed from {dir} — {} deleted, {} edited, {} kept",
        report.deleted(),
        report.excised(),
        report.kept()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::real_which;
    use crate::gui::state::GuiState;
    use crate::run::init_target;
    use crate::testutil::{fake_exec, make_temp_dir};

    fn deps() -> PipelineDeps {
        PipelineDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: real_which(),
            now: None,
        }
    }

    #[test]
    fn removes_ade_from_the_repo_and_stops_tracking_it() {
        let home = make_temp_dir("gui-projects-home");
        let repo = make_temp_dir("gui-projects-repo");
        std::fs::create_dir_all(repo.join(".git/hooks")).expect("git dir");
        std::fs::write(repo.join("CLAUDE.md"), "# mine\n\nnotes\n").expect("claude");

        let pipeline = deps();
        init_target(&repo, &pipeline).expect("init");
        let config = load_config(&repo, &module_ids(), &harness_ids()).expect("config");
        let ctx = make_ctx(&repo, config, &pipeline);
        crate::run::apply_pipeline(&ctx, &pipeline);
        assert!(repo.join("ade.json").exists());

        let dir = repo.to_string_lossy().to_string();
        let mut state = GuiState::default();
        state.projects.push(dir.clone());
        save_gui_state(&home, &state).expect("seed state");

        let message = remove_ade_from_project(&home, &dir, &pipeline).expect("remove");
        assert!(message.contains("deleted"), "{message}");
        assert!(!repo.join("ade.json").exists(), "ade.json must be gone");
        assert!(!repo.join(".ade").exists(), "no empty .ade/ left behind");
        assert_eq!(
            std::fs::read_to_string(repo.join("CLAUDE.md")).expect("claude survives"),
            "# mine\n\nnotes\n",
            "the user's own file must come back exactly"
        );
        assert!(
            load_gui_state(&home).state.projects.is_empty(),
            "the project must no longer be tracked"
        );

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn unknown_or_unbootstrapped_dir_is_an_error_not_a_panic() {
        let home = make_temp_dir("gui-projects-home-bad");
        let empty = make_temp_dir("gui-projects-empty");
        let error = remove_ade_from_project(&home, &empty.to_string_lossy(), &deps())
            .expect_err("a repo with no ade.json cannot be removed from");
        assert!(!error.is_empty());
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
