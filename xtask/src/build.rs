//! `build` — cross-compile the crate in the current directory for the C64
//! (mos-c64-none). Normally invoked as `cargo xbuild`. Passes the mos target +
//! build-std flags explicitly, so it does not depend on any `.cargo/config.toml`
//! default (which cargo discovers by cwd and would leak onto xtask itself).

use std::process::{Command, ExitCode};

pub const HELP: &str = "\
cargo xbuild [CARGO ARGS…]      (also: cargo xtask build)

Cross-compile the crate in the current directory for the C64 (mos-c64-none),
with -Zbuild-std=core,alloc. Runs plain `cargo build` with the mos target and
build-std flags spelled out, so it works regardless of .cargo/config.toml. Run
it inside `nix develop`, from a C64 crate; extra args are forwarded to cargo
(e.g. --release).
";

pub fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [build]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(args: &[String]) -> Result<(), String> {
    // mos target + build-std passed explicitly (see crate::MOS_FLAGS) rather than
    // via an inherited `.cargo/config.toml` default. Extra args forwarded to cargo.
    let status = Command::new("cargo")
        .arg("build")
        .args(crate::MOS_FLAGS)
        .args(args)
        .status()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64 crate)")
        })?;
    if !status.success() {
        return Err("`cargo build` for mos-c64-none failed (see cargo output above)".into());
    }
    Ok(())
}
