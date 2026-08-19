//! `run` — build the crate in the current directory in release and launch it in
//! the emulator for the machine named by `--target`: VICE (`x64sc`) for the C64,
//! Xemu (`xmega65`) for the MEGA65. Normally invoked as `cargo xrun_c64` /
//! `cargo xrun_mega65` (or `cargo xrun`, which is `_c64`); the dev shell exposes
//! these on PATH so they work from any crate directory, while the `cargo xtask`
//! alias is repo-root-only.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, ExitCode, Stdio};

use crate::target::Target;

pub const HELP: &str = "\
cargo xrun --target <TARGET> [CARGO ARGS…]      (also: cargo xtask run)

Build the crate in the current directory in release and launch it in its
emulator. Release by default: a debug build usually won't fit in the machine's
RAM. Runs `cargo run --release` with the mos target + build-std passed
explicitly (so it doesn't depend on .cargo/config.toml); the emulator itself
still comes from that crate's `[target.<triple>] runner`. Run it inside
`nix develop`, from a C64/MEGA65 crate; extra args are forwarded to cargo.

targets:
  c64     Commodore 64 — launches VICE (x64sc -autostart)
  mega65  MEGA65 — launches Xemu (xmega65 -prg)

--target is required; there is no default, because the two machines produce
incompatible binaries. Shorthand aliases (repo root): cargo xrun_c64,
cargo xrun_mega65; plain `cargo xrun` is the same as cargo xrun_c64.

For the C64, VICE's benign 'failed to retrieve executable path' line (Homebrew
macOS, printed to stdout during arch-init) is filtered from the output.
";

pub fn run(args: &[String]) -> ExitCode {
    let (target, cargo_args) = match crate::target::split_or_usage(args, "run") {
        Ok(v) => v,
        Err(code) => return code,
    };
    match run_inner(target, &cargo_args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [run]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(target: Target, args: &[String]) -> Result<(), String> {
    // Build in release and launch it. Release by default because a debug build
    // usually won't fit in the machine's RAM. The mos target + build-std are
    // passed explicitly (Target::cargo_flags), not inherited from
    // `.cargo/config.toml`; the emulator still comes from that config's
    // `[target.<triple>] runner` (inert for host builds). Run it inside
    // `nix develop`. Extra args are forwarded verbatim to cargo.
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--release"])
        .args(target.cargo_flags())
        .args(args);

    // Only the C64 path needs stdout piped: that filtering exists solely to drop
    // VICE's benign arch-init noise (see is_vice_arch_noise). Xemu has no such
    // quirk, so its stdout stays inherited and untouched. cargo writes its build
    // progress and real errors to *stderr*, which is inherited either way — so
    // filtering touches only the x64sc runner's output, never cargo's.
    let status = match target {
        Target::Mega65 => cmd
            .status()
            .map_err(|e| spawn_err(e.to_string()))?,
        Target::C64 => {
            let mut child = cmd
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|e| spawn_err(e.to_string()))?;
            let stdout = child.stdout.take().expect("child stdout was set to piped");
            let mut out = std::io::stdout();
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(|e| format!("reading runner stdout: {e}"))?;
                if is_vice_arch_noise(&line) {
                    continue;
                }
                writeln!(out, "{line}").map_err(|e| format!("writing to stdout: {e}"))?;
            }
            child
                .wait()
                .map_err(|e| format!("waiting for `cargo run`: {e}"))?
        }
    };

    if !status.success() {
        return Err(format!(
            "`cargo run --release` for {} failed (see cargo output above)",
            target.triple()
        ));
    }
    Ok(())
}

fn spawn_err(e: String) -> String {
    format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64/MEGA65 crate)")
}

/// True for VICE's benign "can't locate my own executable" messages, which
/// Homebrew's macOS x64sc (3.10) prints to *stdout* during arch-init:
///
/// ```text
/// Error - failed to retrieve executable path, falling back to getcwd() + argv[0]
/// Error - argv[0] is NULL, giving up.
/// ```
///
/// VICE wants its executable path to find its ROMs relative to itself. This is a
/// bug in the Homebrew macOS **VICE 3.10** build: its macOS branch calls
/// `proc_pidpath(pid, buffer, sizeof(buffer) - 1)` with the buffer one byte under
/// `proc_pidpath`'s minimum, so the call fails every time and VICE announces the
/// fallback. Harmless (VICE still locates its ROMs) and unsuppressible by flags —
/// the line is emitted before VICE's log system exists, so neither `-silent` nor
/// `-loglimit 0` touches it, and it fires regardless of how x64sc is invoked
/// (absolute path, bare name, from its own bin dir — all reproduce it) because
/// `proc_pidpath` ignores cwd/argv[0]. So we filter these two exact lines and pass
/// everything else through unchanged.
///
/// REMOVE THIS once a fixed VICE stable ships (VICE `main` already sizes the
/// buffer to 4096; no stable release has it yet, and none newer than 3.10 exists
/// in Homebrew). Then `brew upgrade vice`, delete this predicate, and drop the
/// `Target::C64` arm's piped-stdout branch in `run_inner` for a plain `.status()`.
fn is_vice_arch_noise(line: &str) -> bool {
    let l = line.trim();
    l.contains("failed to retrieve executable path")
        || l.contains("argv[0] is NULL, giving up")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_only_vice_arch_noise() {
        // The two benign arch-init lines are dropped...
        assert!(is_vice_arch_noise(
            "Error - failed to retrieve executable path, falling back to getcwd() + argv[0]"
        ));
        assert!(is_vice_arch_noise("Error - argv[0] is NULL, giving up."));
        // ...tolerating leading/trailing whitespace...
        assert!(is_vice_arch_noise(
            "  Error - argv[0] is NULL, giving up.  "
        ));
        // ...while real output — including other VICE errors — passes through.
        assert!(!is_vice_arch_noise(""));
        assert!(!is_vice_arch_noise("Error - cannot load kernal ROM"));
        assert!(!is_vice_arch_noise("Hello from the C64!"));
    }
}
