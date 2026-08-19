//! `check` — type-check the crate in the current directory for the machine
//! named by `--target`. Normally invoked as `cargo xcheck_c64` /
//! `cargo xcheck_mega65` (or `cargo xcheck`, which is `_c64`). Passes the mos
//! target + build-std flags explicitly, so it does not depend on any
//! `.cargo/config.toml` default (which cargo discovers by cwd and would leak
//! onto xtask itself).

use std::process::{Command, ExitCode};

pub const HELP: &str = "\
cargo xcheck --target <TARGET> [CARGO ARGS…]    (also: cargo xtask check)

Type-check the crate in the current directory, with -Zbuild-std=core,alloc.
Runs plain `cargo check` with the mos target and build-std flags spelled out,
so it works regardless of .cargo/config.toml. Run it inside `nix develop`, from
a C64/MEGA65 crate; extra args are forwarded to cargo.

targets:
  c64     Commodore 64 — 6502, mos-c64-none
  mega65  MEGA65 — 45GS02, mos-mega65-none

--target is required; there is no default, because the two machines produce
incompatible binaries. Shorthand aliases (repo root): cargo xcheck_c64,
cargo xcheck_mega65; plain `cargo xcheck` is the same as cargo xcheck_c64.
";

pub fn run(args: &[String]) -> ExitCode {
    let (target, cargo_args) = match crate::target::split_or_usage(args, "check") {
        Ok(v) => v,
        Err(code) => return code,
    };
    match run_inner(target, &cargo_args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [check]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(target: crate::target::Target, args: &[String]) -> Result<(), String> {
    let status = Command::new("cargo")
        .arg("check")
        .args(target.cargo_flags())
        .args(args)
        .status()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64/MEGA65 crate)")
        })?;
    if !status.success() {
        return Err(format!(
            "`cargo check` for {} failed (see cargo output above)",
            target.triple()
        ));
    }
    Ok(())
}
