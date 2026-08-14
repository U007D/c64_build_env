//! `run` — build the crate in the current directory in release and launch it in
//! the VICE emulator. Normally invoked as `cargo xrun` (the dev shell exposes it
//! on PATH so it works from any crate directory; the `cargo xtask` alias is
//! repo-root-only).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, ExitCode, Stdio};

pub const HELP: &str = "\
cargo xrun [CARGO ARGS…]        (also: cargo xtask run)

Build the crate in the current directory in release and launch it in VICE.
Release by default: a debug C64 build usually won't fit in the machine's RAM.
Runs `cargo run --release` with the mos target + build-std passed explicitly
(so it doesn't depend on .cargo/config.toml); the x64sc runner still comes from
the crate's `[target.mos-c64-none] runner`. Run it inside `nix develop`, from a
C64 crate; extra args are forwarded to cargo.

VICE's benign 'failed to retrieve executable path' line (Homebrew macOS, printed
to stdout during arch-init) is filtered from the output.
";

pub fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [run]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(args: &[String]) -> Result<(), String> {
    // Build in release and launch it. Release by default because a debug C64
    // build usually won't fit in the machine's RAM. The mos target + build-std
    // are passed explicitly (crate::MOS_FLAGS), not inherited from
    // `.cargo/config.toml`; the x64sc runner still comes from that config's
    // `[target.mos-c64-none] runner` (inert for host builds). Run it inside
    // `nix develop`, from a C64 crate. Extra args are forwarded verbatim to cargo.
    //
    // We pipe the child's *stdout* so we can drop VICE's benign arch-init noise
    // (see is_vice_arch_noise). cargo writes its build progress and real errors
    // to *stderr*, which we leave inherited — so filtering stdout touches only
    // the x64sc runner's output, never cargo's.
    let mut child = Command::new("cargo")
        .args(["run", "--release"])
        .args(crate::MOS_FLAGS)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a C64 crate)")
        })?;

    let stdout = child.stdout.take().expect("child stdout was set to piped");
    let mut out = std::io::stdout();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| format!("reading runner stdout: {e}"))?;
        if is_vice_arch_noise(&line) {
            continue;
        }
        writeln!(out, "{line}").map_err(|e| format!("writing to stdout: {e}"))?;
    }

    let status = child
        .wait()
        .map_err(|e| format!("waiting for `cargo run`: {e}"))?;
    if !status.success() {
        return Err("`cargo run --release` for mos-c64-none failed (see cargo output above)".into());
    }
    Ok(())
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
/// in Homebrew). Then `brew upgrade vice`, delete this predicate, and restore
/// `run_inner` to a plain inherited-stdout `.status()` call.
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
