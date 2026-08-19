//! `build` — cross-compile the crate in the current directory for the machine
//! named by `--target`. Normally invoked as `cargo xbuild_c64` /
//! `cargo xbuild_mega65` (or `cargo xbuild`, which is `_c64`). Passes the mos
//! target + build-std flags explicitly, so it does not depend on any
//! `.cargo/config.toml` default (which cargo discovers by cwd and would leak
//! onto xtask itself).

use std::process::{Command, ExitCode};

pub const HELP: &str = "\
cargo xbuild --target <TARGET> [CARGO ARGS…]    (also: cargo xtask build)

Cross-compile the crate in the current directory, with -Zbuild-std=core,alloc.
Runs plain `cargo build` with the mos target and build-std flags spelled out,
so it works regardless of .cargo/config.toml. Run it inside `nix develop`, from
a C64/MEGA65 crate; extra args are forwarded to cargo (e.g. --release).

targets:
  c64     Commodore 64 — 6502, mos-c64-none
  mega65  MEGA65 — 45GS02, mos-mega65-none

--target is required; there is no default, because the two machines produce
incompatible binaries. Shorthand aliases (repo root): cargo xbuild_c64,
cargo xbuild_mega65; plain `cargo xbuild` is the same as cargo xbuild_c64.
";

pub fn run(args: &[String]) -> ExitCode {
    let (target, cargo_args) = match crate::target::split_or_usage(args, "build") {
        Ok(v) => v,
        Err(code) => return code,
    };
    match run_inner(target, &cargo_args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [build]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(target: crate::target::Target, args: &[String]) -> Result<(), String> {
    // mos target + build-std passed explicitly (see Target::cargo_flags) rather
    // than via an inherited `.cargo/config.toml` default. Extra args forwarded.
    let status = Command::new("cargo")
        .arg("build")
        .args(target.cargo_flags())
        .args(args)
        .status()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64/MEGA65 crate)")
        })?;
    if !status.success() {
        return Err(format!(
            "`cargo build` for {} failed (see cargo output above)",
            target.triple()
        ));
    }
    Ok(())
}
