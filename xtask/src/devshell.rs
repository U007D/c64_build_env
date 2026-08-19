//! Warns when the surrounding `nix develop` shell is missing or out of date.
//!
//! Everything the dev shell provides — `RUST_TARGET_PATH`, `PATH`, the
//! `cargo-x*` shims — is fixed when the shell starts. Editing `flake.nix`
//! afterwards changes nothing until you re-enter, and the symptom is usually
//! something unrelated-looking, e.g.
//!
//! ```text
//! error: Error loading target specification: Could not find specification for
//! target "mos-mega65-none"
//! ```
//!
//! So the checks are cheap and advisory: two file hashes compared against the
//! stamp `flake.nix` exported at entry. Never fatal — the build may well be fine
//! (a stale shell only matters if the edit touched something you need).

use std::path::{Path, PathBuf};

const STAMP_VAR: &str = "XTASK_DEVSHELL_STAMP";

/// Fail if we are not in a dev shell; warn if we are in one older than the
/// current `flake.nix` / `flake.lock`. Silent when everything matches, or when
/// staleness cannot be determined confidently.
///
/// Missing is fatal, stale is not: without the dev shell these commands cannot
/// work at all (`-Zbuild-std` needs the forked cargo, and the mos targets are on
/// `RUST_TARGET_PATH`), whereas a stale shell usually still builds — it only
/// matters if the edit touched something you need.
pub(crate) fn check() -> Result<(), String> {
    let stamp = match std::env::var(STAMP_VAR) {
        Ok(s) => s,
        Err(_) => {
            return Err(
                "the `rust-mos` and `llvm-mos` toolchains are required.\n       \
                 Make them available with `nix develop` (or `nix develop -c <your preferred shell>`)."
                    .to_string(),
            );
        }
    };

    // Locate the flake by walking up from the cwd, not from this binary: the
    // cargo-x* shims are built into the nix store, so CARGO_MANIFEST_DIR would
    // point there rather than at the repo.
    let Some(dir) = find_flake_dir() else {
        return Ok(()); // not under a flake; nothing to compare against
    };
    let Some(current) = stamp_for(&dir) else {
        return Ok(()); // hashing failed; stay quiet rather than cry wolf
    };

    if current != stamp {
        eprintln!(
            "warning: this `nix develop` shell predates the current flake \
             ({}/flake.nix changed since it started).",
            dir.display()
        );
        eprintln!(
            "         Its RUST_TARGET_PATH/PATH are stale — exit and re-enter `nix develop` \
             if something is unexpectedly missing."
        );
    }
    Ok(())
}

/// Nearest ancestor of the cwd containing a `flake.nix`.
fn find_flake_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("flake.nix").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// `<sha256 of flake.nix>:<sha256 of flake.lock>` — the same string flake.nix
/// builds with `builtins.hashFile`, which also emits lowercase base16.
fn stamp_for(dir: &Path) -> Option<String> {
    let flake = crate::sha256_file(&dir.join("flake.nix")).ok()?;
    let lock = dir.join("flake.lock");
    let lock = if lock.is_file() {
        crate::sha256_file(&lock).ok()?
    } else {
        "nolock".to_string()
    };
    Some(format!("{flake}:{lock}"))
}
