//! `check` — type-check the crate in the current directory for the C64
//! (mos-c64-none). Normally invoked as `cargo xcheck`. Passes the mos target +
//! build-std flags explicitly, so it does not depend on any `.cargo/config.toml`
//! default (which cargo discovers by cwd and would leak onto xtask itself).

use std::process::{Command, ExitCode};

pub const HELP: &str = "\
cargo xcheck [CARGO ARGS…]      (also: cargo xtask check)

Type-check the crate in the current directory for the C64 (mos-c64-none), with
-Zbuild-std=core,alloc. Runs plain `cargo check` with the mos target and
build-std flags spelled out, so it works regardless of .cargo/config.toml. Run
it inside `nix develop`, from a C64 crate; extra args are forwarded to cargo.
";

pub fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [check]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(args: &[String]) -> Result<(), String> {
    let status = Command::new("cargo")
        .arg("check")
        .args(crate::MOS_FLAGS)
        .args(args)
        .status()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64 crate)")
        })?;
    if !status.success() {
        return Err("`cargo check` for mos-c64-none failed (see cargo output above)".into());
    }
    Ok(())
}
