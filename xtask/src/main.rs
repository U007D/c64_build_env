//! Repo tasks, cargo-xtask style: `cargo xtask <task>` (alias wired in
//! .cargo/config.toml; run from the repo root). Each task lives in its own
//! module; the [`COMMANDS`] table is the single source of truth for dispatch,
//! the `usage` summary, and per-task `--help`. Adding a task is a new module
//! plus one row here — the three never drift apart.
//!
//! Cross-task plumbing — locating and driving `nix`, plus small digest helpers —
//! lives here in the crate root (`pub(crate)`), shared by the initenv, prefetch,
//! and publish-toolchain-binaries tasks.

mod asm;
mod build;
mod check;
mod initenv;
mod prefetch;
mod publish_toolchain_binaries;
mod run;
mod target;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

// The per-machine cross-compile flags that used to live here now come from
// `target::Target::cargo_flags()`, since build/check/run/asm each select a
// machine with a required `--target c64|mega65`.

// ---------------------------------------------------------------------------
// dispatch
// ---------------------------------------------------------------------------

/// One subcommand: its name, a one-line summary for `usage`, a longer `--help`
/// blurb (each task's module owns its `HELP`), and its entry point.
struct Cmd {
    name: &'static str,
    summary: &'static str,
    help: &'static str,
    run: fn(&[String]) -> ExitCode,
}

const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "initenv",
        summary: "install Nix if needed, then build + check the flake",
        help: initenv::HELP,
        run: initenv::run,
    },
    Cmd {
        name: "publish-toolchain-binaries",
        summary: "build the toolchain and push it to a Cachix cache",
        help: publish_toolchain_binaries::HELP,
        run: publish_toolchain_binaries::run,
    },
    Cmd {
        name: "prefetch-hashes",
        summary: "pin the PREFETCH placeholder hashes in toolchain/pins.nix",
        help: prefetch::HELP,
        run: prefetch::run,
    },
    Cmd {
        name: "asm",
        summary: "show the mos assembly for the crate in the current directory",
        help: asm::HELP,
        run: asm::run,
    },
    Cmd {
        name: "run",
        summary: "build the current crate in release and launch it in its emulator",
        help: run::HELP,
        run: run::run,
    },
    Cmd {
        name: "build",
        summary: "cross-compile the current crate for --target c64|mega65",
        help: build::HELP,
        run: build::run,
    },
    Cmd {
        name: "check",
        summary: "type-check the current crate for --target c64|mega65",
        help: check::HELP,
        run: check::run,
    },
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(name) => match COMMANDS.iter().find(|c| c.name == name) {
            // `cargo xtask <task> --help` prints the task's own blurb; anything
            // else (including a later `--help`) is the task's to interpret.
            Some(cmd) if args.get(1).map(String::as_str) == Some("--help") => {
                print!("{}", cmd.help);
                ExitCode::SUCCESS
            }
            Some(cmd) => (cmd.run)(&args[1..]),
            None => {
                eprintln!("unknown task: {name}");
                usage();
                ExitCode::FAILURE
            }
        },
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!("usage: cargo xtask <task>");
    eprintln!();
    eprintln!("tasks:");
    // Align every summary to one column, sized to the longest task name.
    let width = COMMANDS.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in COMMANDS {
        eprintln!("  {:<width$}  {}", c.name, c.summary);
    }
    eprintln!();
    eprintln!("Run `cargo xtask <task> --help` for details.");
}

/// The repo root is one level up from this crate's manifest — never derived
/// from the cwd, so tasks behave the same from any invocation directory.
pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("cannot canonicalize repo root")
}

// ---------------------------------------------------------------------------
// nix invocation helpers (shared by initenv / prefetch / publish-toolchain-binaries)
// ---------------------------------------------------------------------------

/// Global flags threaded onto every `nix` call so tasks work even before flakes
/// is persistently enabled.
const NIX_EXPERIMENTAL: [&str; 2] = ["--extra-experimental-features", "nix-command flakes"];

/// Locate a usable `nix`: on PATH, else the multi-user default profile, else
/// the single-user profile. Returns the bare name `nix` when it's on PATH
/// (Command resolves it), otherwise an absolute path.
pub(crate) fn find_nix() -> Option<PathBuf> {
    if let Ok(out) = Command::new("nix").arg("--version").output() {
        if out.status.success() {
            return Some(PathBuf::from("nix"));
        }
    }
    let default_profile = PathBuf::from("/nix/var/nix/profiles/default/bin/nix");
    if default_profile.exists() {
        return Some(default_profile);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_profile = PathBuf::from(home).join(".nix-profile/bin/nix");
        if user_profile.exists() {
            return Some(user_profile);
        }
    }
    None
}

/// sha256 of a file as lowercase hex, via the platform digest tool (keeps the
/// crate dependency-free).
pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let (prog, prog_args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("shasum", &["-a", "256"])
    } else {
        ("sha256sum", &[])
    };
    let out = Command::new(prog)
        .args(prog_args)
        .arg(path)
        .output()
        .map_err(|e| format!("failed to spawn {prog}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{prog} failed on {}", path.display()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_sha256_line(&stdout)
        .ok_or_else(|| format!("could not parse {prog} output: {}", stdout.trim()))
}

/// The 64-hex digest is the first whitespace-delimited field of `shasum` /
/// `sha256sum` output (`<hex>  <file>`), returned lowercased.
fn parse_sha256_line(s: &str) -> Option<String> {
    let first = s.split_whitespace().next()?;
    if first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(first.to_ascii_lowercase())
    } else {
        None
    }
}

/// A `nix` Command with the experimental-features flags prepended and the cwd
/// set to the repo root. Callers choose how to wire stdio.
pub(crate) fn nix_command(nix: &Path, root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(nix);
    cmd.args(NIX_EXPERIMENTAL).args(args).current_dir(root);
    // Right after a fresh install (this same process), NIX_SSL_CERT_FILE isn't set
    // yet — that only happens in a new shell. Nix then falls back to its built-in
    // cert search, which can hit a stale `/etc/ssl/certs/ca-certificates.crt`
    // (e.g. a dangling symlink left by a removed nix-darwin) and fail every fetch.
    // Point it at the valid bundle Nix just installed, unless the user set one.
    if let Some(cacert) = nix_cacert(nix) {
        cmd.env("NIX_SSL_CERT_FILE", cacert);
    }
    cmd
}

/// The CA bundle Nix installs next to itself, used as a fallback `NIX_SSL_CERT_FILE`.
/// Returns `None` if the user already set the variable (respect their choice) or
/// no bundle is found.
fn nix_cacert(nix: &Path) -> Option<PathBuf> {
    if std::env::var_os("NIX_SSL_CERT_FILE").is_some() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // Derived from an absolute nix path: <prefix>/bin/nix -> <prefix>/etc/ssl/…
    if let Some(prefix) = nix.parent().and_then(Path::parent) {
        candidates.push(prefix.join("etc/ssl/certs/ca-bundle.crt"));
    }
    // The standard multi-user profile location.
    candidates.push(PathBuf::from(
        "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
    ));
    candidates.into_iter().find(|p| p.exists())
}

/// Run `nix …`, capturing combined stdout+stderr (hash-mismatch reports go to
/// stderr). Returns (success, combined output).
pub(crate) fn nix_run_capture(
    nix: &Path,
    root: &Path,
    args: &[&str],
) -> Result<(bool, String), String> {
    let out = nix_command(nix, root, args)
        .output()
        .map_err(|e| format!("failed to spawn `nix`: {e} (is Nix installed and on PATH?)"))?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), combined))
}

/// Run `nix …` with inherited stdio (so long build logs stream live). Returns
/// whether it succeeded.
pub(crate) fn run_nix_inherit(nix: &Path, root: &Path, args: &[&str]) -> Result<bool, String> {
    let status = nix_command(nix, root, args)
        .status()
        .map_err(|e| format!("failed to spawn `nix`: {e}"))?;
    Ok(status.success())
}

/// Build `.#rust-mos`, streaming logs (stderr) live while capturing the printed
/// out-path (stdout). Returns the trimmed store path.
pub(crate) fn build_toolchain(nix: &Path, root: &Path, extra: &[&str]) -> Result<String, String> {
    let mut args = vec!["build", ".#rust-mos", "--print-out-paths"];
    args.extend_from_slice(extra);
    let out = nix_command(nix, root, &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn `nix`: {e}"))?
        .wait_with_output()
        .map_err(|e| format!("waiting on `nix`: {e}"))?;
    if !out.status.success() {
        return Err("`nix build .#rust-mos` failed".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sha256_line() {
        let hex = "82723616373d0c3f0d07b892f5f5c023da825b8969a2351c7055926d0bcf5553";
        assert_eq!(
            parse_sha256_line(&format!("{hex}  nix-installer-aarch64-darwin")).unwrap(),
            hex
        );
        // uppercase is normalized to lowercase
        assert_eq!(parse_sha256_line(&hex.to_ascii_uppercase()).unwrap(), hex);
        // too short / non-hex are rejected
        assert!(parse_sha256_line("deadbeef  file").is_none());
        assert!(parse_sha256_line("").is_none());
        assert!(parse_sha256_line(&format!("{}  file", "z".repeat(64))).is_none());
    }
}
