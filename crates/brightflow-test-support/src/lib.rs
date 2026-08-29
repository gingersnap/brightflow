//! Copy the committed public test workspace into a throwaway `TempDir` so
//! storage-backed integration tests boot against the same data the interactive
//! env does.
//!
//! Brightflow treats *a workspace folder* as the unit of isolation, and that is
//! the unit of test too. The repo commits a single small, statically-curated
//! template folder (`testdata/workspaces/test/`) — the "public test set" — and
//! this crate is the one shared copy step. Both the ephemeral suite and the
//! persistent interactive env (§3.2/§3.3 of the test-workspace plan) derive from
//! that one template, so there is a single source of truth and no per-version
//! ceremony: any schema drift is picked up by the existing self-migrate-on-open
//! machinery.
//!
//! This crate deliberately knows *nothing* about the data it copies. It copies
//! bytes (`fs`) and hands back a `WorkspacePaths`; it never reads or rebuilds a
//! schema. Generation was ruled out — "copy, not generate" is the whole point —
//! so there is no schema knowledge here to drift. The template is built by a
//! defined one-time procedure (see `scripts/test-env.sh` / the plan §4), not by
//! this crate.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use brightflow_core::WorkspacePaths;

/// Env var pointing at the committed template folder.
///
/// The value is the `test` workspace dir itself; overrides the repo-default
/// location so a run can resolve the template from anywhere (CI, a vendored
/// copy) without touching source.
pub const TEMPLATE_ENV: &str = "BRIGHTFLOW_TESTDATA_DIR";

/// The workspace name inside the template's `workspaces/` tree.
///
/// The committed folder is `testdata/workspaces/test/`, and every copy keeps
/// that name so the same `WorkspacePaths` path always resolves:
/// `{tmp}/workspaces/test/...`.
pub const TEMPLATE_WORKSPACE: &str = "test";

/// A fresh copy of the committed template, kept alive for the enclosing scope.
///
/// `TempDir` deletes itself on drop, so returning it (not just its path) is
/// what guarantees the ephemeral copy is discarded with the test run. `paths`
/// is already wired to this copy, so a test boots the app/store against it
/// exactly as it boots production (`WorkspacePaths` + self-migrate on open).
pub struct TestWorkspace {
    /// Kept alive for the enclosing scope: dropping this field (via `TempDir`
    /// RAII) deletes the copied workspace. Never read by name — that is the
    /// point — so the underscore name is deliberate.
    _dir: tempfile::TempDir,
    /// Workspace paths resolved onto the fresh copy.
    pub paths: WorkspacePaths,
    /// The copied workspace root (`{tmp}/workspaces/test`). Cached here (not
    /// re-derived from `paths.root()`) so `root()` can lend out a `&Path`.
    root: PathBuf,
}

impl TestWorkspace {
    /// The copied workspace root (`{tmp}/workspaces/test`).
    ///
    /// Useful for assertions that walk the filesystem; data access should go
    /// through `paths` so tests exercise the same resolution production uses.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Resolve the committed template folder (the `test` workspace dir itself),
/// honoring the `BRIGHTFLOW_TESTDATA_DIR` override.
#[must_use]
pub fn template_workspace_dir() -> PathBuf {
    std::env::var(TEMPLATE_ENV).map_or_else(|_| default_template_workspace_dir(), PathBuf::from)
}

/// Default template location: repo-root `testdata/workspaces/test`.
///
/// Derived from this crate's manifest dir (`crates/brightflow-test-support` →
/// two hops up is the repo root) so it works regardless of the process CWD —
/// `cargo test` runs from the crate directory, not the repo root.
#[must_use]
pub fn default_template_workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join("workspaces")
        .join(TEMPLATE_WORKSPACE)
}

/// Copy the committed template into a fresh throwaway `TempDir`.
///
/// The copy keeps the `workspaces/test` nesting of the source so the result is
/// a valid data dir: `{tmp}/workspaces/test/...`. Returns an error describing
/// where it looked if the template is missing — that is a repo/setup problem,
/// not something a test should silently pass over.
pub fn copy_template() -> io::Result<TestWorkspace> {
    let template = template_workspace_dir();
    if !template.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "test workspace template not found at {} (set {} to override); \
                 run `scripts/test-env.sh setup` or rebuild it per the plan §4",
                template.display(),
                TEMPLATE_ENV
            ),
        ));
    }

    // The template's parent is the `workspaces/` dir. Copy that dir *as a
    // child* of the temp dir (dst = `{tmp}/workspaces`), which reproduces the
    // `{tmp}/workspaces/test` layout so `WorkspacePaths::new(tmp, "test")`
    // resolves to the copied workspace.
    let workspaces_dir = template
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "template has no parent dir"))?;
    let dir = tempfile::tempdir()?;
    let dst = dir.path().join("workspaces");
    copy_tree(workspaces_dir, &dst)?;

    let paths = WorkspacePaths::new(dir.path(), TEMPLATE_WORKSPACE);
    let root = paths.root();
    Ok(TestWorkspace {
        _dir: dir,
        paths,
        root,
    })
}

/// Recursively copy `from` into `to`, creating `to` if needed.
///
/// Pure byte copy — no filtering, no schema awareness. Hidden files and any
/// stray `-wal`/`-shm` sidecars are copied like any other file; the committed
/// template is expected to be checkpointed and sidecar-free, and the
/// `.gitignore` guards make a stray sidecar un-committable, but the helper
/// doesn't need to know about SQLite to be a faithful copy.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let dir_entry = entry?;
        let src = dir_entry.path();
        let dst = to.join(dir_entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `copy_tree` reproduces the whole tree, byte-for-byte, including nested
    /// dirs and files that are not logs. (A stray sidecar is copied like any
    /// other file — filtering is a git-hygiene concern, not a copy concern.)
    #[test]
    fn copy_tree_mirrors_source_faithfully() {
        let src = tempfile::tempdir().unwrap();
        fs::create_dir_all(src.path().join("workspaces/test/store")).unwrap();
        fs::write(
            src.path().join("workspaces/test/store/a.parquet"),
            b"parquet-bytes",
        )
        .unwrap();
        fs::write(src.path().join("workspaces/test/litehouse.db"), b"db-bytes").unwrap();
        // A transient sidecar must survive the copy byte-for-byte too.
        fs::write(
            src.path().join("workspaces/test/litehouse.db-wal"),
            b"wal-bytes",
        )
        .unwrap();

        let dst = tempfile::tempdir().unwrap();
        copy_tree(src.path(), dst.path()).unwrap();

        let copied_db = dst.path().join("workspaces/test/litehouse.db");
        assert_eq!(fs::read(&copied_db).unwrap(), b"db-bytes");
        assert_eq!(
            fs::read(dst.path().join("workspaces/test/store/a.parquet")).unwrap(),
            b"parquet-bytes"
        );
        assert_eq!(
            fs::read(dst.path().join("workspaces/test/litehouse.db-wal")).unwrap(),
            b"wal-bytes"
        );
    }

    /// The default template path points into this repo's `testdata/`, derived
    /// from the manifest dir (not the process CWD).
    #[test]
    fn default_template_dir_resolves_into_repo_testdata() {
        let dir = default_template_workspace_dir();
        assert!(
            dir.ends_with("testdata/workspaces/test"),
            "unexpected default path: {}",
            dir.display()
        );
    }

    /// `BRIGHTFLOW_TESTDATA_DIR` overrides the template location. This test
    /// owns that env var; a static lock serializes it against the other env
    /// tests, because env writes are process-global and cargo runs tests in a
    /// binary concurrently.
    #[test]
    fn env_override_points_at_alternate_template() {
        let _guard = env_lock();
        let fake = tempfile::tempdir().unwrap();
        let template = fake.path().join("workspaces/test");
        fs::create_dir_all(&template).unwrap();
        fs::write(template.join("litehouse.db"), b"x").unwrap();

        std::env::set_var(TEMPLATE_ENV, &template);
        assert_eq!(template_workspace_dir(), template);
        std::env::remove_var(TEMPLATE_ENV);
    }

    /// `copy_template` produces a valid data dir: `{tmp}/workspaces/test` with
    /// the copied bytes, and `paths` resolves into it.
    #[test]
    fn copy_template_builds_a_wired_workspace() {
        let _guard = env_lock();
        let fake = tempfile::tempdir().unwrap();
        let template = fake.path().join("workspaces/test");
        fs::create_dir_all(&template).unwrap();
        fs::write(template.join("litehouse.db"), b"template-db").unwrap();

        std::env::set_var(TEMPLATE_ENV, &template);

        let ws = copy_template().unwrap();
        assert!(ws.root().ends_with("workspaces/test"));
        assert_eq!(
            fs::read(ws.root().join("litehouse.db")).unwrap(),
            b"template-db"
        );
        assert_eq!(
            ws.paths.root(),
            ws.root().to_path_buf(),
            "paths must resolve onto the copy itself"
        );

        std::env::remove_var(TEMPLATE_ENV);
    }

    /// A missing template is a hard, descriptive error — not a silent pass.
    #[test]
    fn missing_template_errors_with_location() {
        let _guard = env_lock();
        let empty = tempfile::tempdir().unwrap();
        std::env::set_var(TEMPLATE_ENV, empty.path().join("nowhere"));

        let err = copy_template().err().expect("copy must fail");
        assert!(err.to_string().contains("template not found"));

        std::env::remove_var(TEMPLATE_ENV);
    }

    /// Serialize the env-touching tests against each other: they all RMW the
    /// same process-global var, which is a race across parallel test threads.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
