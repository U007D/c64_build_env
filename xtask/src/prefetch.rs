//! `prefetch-hashes` — pin the `PREFETCH:` placeholder hashes in
//! toolchain/pins.nix. The standard fixed-output-derivation workflow, automated:
//! for each target, `nix build .#<target>` with the placeholder hash; Nix reports
//! `specified: … got: …`; we substitute the reported hash and re-run to verify.
//! Run once, on a machine with network access; idempotent (already-pinned targets
//! are skipped).

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::{find_nix, nix_run_capture, repo_root};

pub const HELP: &str = "\
cargo xtask prefetch-hashes

Pin the PREFETCH placeholder hashes in toolchain/pins.nix (runs `nix build`,
needs network; idempotent). Run once, on a machine with network access.
";

/// Flake attributes to prefetch, cheap source fetches first, then the
/// expensive vendor FODs. Each must have a matching `# PREFETCH:<name>`
/// marker in toolchain/pins.nix.
const TARGETS: [&str; 5] = [
    "llvm-mos-source",
    "llvm-mos-sdk-source",
    "rust-mos-src",
    "check-vendor",
    "example-vendor",
];

/// `lib.fakeHash`: what an unpinned entry looks like, byte for byte.
pub(crate) const PLACEHOLDER: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("prefetch-hashes takes no arguments");
        return ExitCode::FAILURE;
    }
    let root = repo_root();
    let pins_path = root.join("toolchain/pins.nix");
    if !pins_path.is_file() {
        eprintln!("ERROR: {} not found", pins_path.display());
        return ExitCode::FAILURE;
    }
    let nix = match find_nix() {
        Some(n) => n,
        None => {
            eprintln!("ERROR: Nix not found. Install it or run `cargo xtask initenv`.");
            return ExitCode::FAILURE;
        }
    };

    match pin_all(&nix, &root, &pins_path) {
        Ok(_) => {
            println!();
            println!("All hashes pinned. Next:");
            println!("  nix flake check   # builds llvm-mos + rust-mos + the offline C64 PRG check");
            println!("  nix develop       # mos toolchain shell");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Pin every target in `TARGETS`. Returns whether any were newly pinned.
pub(crate) fn pin_all(nix: &Path, root: &Path, pins_path: &Path) -> Result<bool, String> {
    let mut changed = false;
    for target in TARGETS {
        if pin_target(nix, root, pins_path, target)? {
            changed = true;
        }
    }
    Ok(changed)
}

/// Pin one target. Returns whether it was newly pinned (false = already done).
fn pin_target(nix: &Path, root: &Path, pins_path: &Path, target: &str) -> Result<bool, String> {
    let marker = format!("PREFETCH:{target}");
    let contents = fs::read_to_string(pins_path).map_err(|e| format!("reading pins.nix: {e}"))?;

    let line_idx = find_marker_line(&contents, &marker)?;
    if !contents.lines().nth(line_idx).unwrap().contains(PLACEHOLDER) {
        println!("== {target}: already pinned, skipping");
        return Ok(false);
    }

    let spec = format!(".#{target}");
    println!("== {target}: running `nix build {spec}` (expected to fail with a hash mismatch)…");
    let (success, output) = nix_run_capture(nix, root, &["build", spec.as_str(), "--no-link"])?;
    if success {
        return Err(
            "build unexpectedly SUCCEEDED with the placeholder hash — refusing to continue".into(),
        );
    }

    let got = extract_got_hash(&output).ok_or_else(|| {
        format!(
            "no `got: sha256-…` in nix output — a real build error, not a hash mismatch.\n\
             ---- captured output ----\n{output}\n-------------------------"
        )
    })?;

    let patched = replace_on_line(&contents, line_idx, PLACEHOLDER, &got)?;
    fs::write(pins_path, patched).map_err(|e| format!("writing pins.nix: {e}"))?;
    println!("== {target}: pinned {got}");

    println!("== {target}: verifying (re-running the build)…");
    let (success, output) = nix_run_capture(nix, root, &["build", spec.as_str(), "--no-link"])?;
    if !success {
        return Err(format!(
            "verification build FAILED after pinning.\n\
             ---- captured output ----\n{output}\n-------------------------"
        ));
    }
    println!("== {target}: verified");
    Ok(true)
}

/// Index of the single line carrying `marker`; ambiguity is an error.
fn find_marker_line(contents: &str, marker: &str) -> Result<usize, String> {
    let hits: Vec<usize> = contents
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(marker))
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("marker '{marker}' not found in pins.nix")),
        many => Err(format!(
            "marker '{marker}' found on {} lines — pins.nix is malformed",
            many.len()
        )),
    }
}

/// Replace `from` with `to`, only on line `idx`. Preserves the trailing
/// newline state of the file.
fn replace_on_line(contents: &str, idx: usize, from: &str, to: &str) -> Result<String, String> {
    let mut lines: Vec<&str> = contents.lines().collect();
    let line = lines
        .get(idx)
        .ok_or_else(|| format!("line {idx} out of range"))?;
    if !line.contains(from) {
        return Err(format!("line {idx} no longer contains the placeholder"));
    }
    let replaced = line.replacen(from, to, 1);
    lines[idx] = &replaced;
    let mut out = lines.join("\n");
    if contents.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// Find the LAST `got:` in nix's output and return the `sha256-…` SRI token
/// that follows it (44 base64 chars after the prefix).
fn extract_got_hash(output: &str) -> Option<String> {
    let is_b64 = |c: char| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=';
    let mut result = None;
    let mut rest = output;
    while let Some(pos) = rest.find("got:") {
        let after = rest[pos + 4..].trim_start();
        if let Some(stripped) = after.strip_prefix("sha256-") {
            let b64: String = stripped.chars().take_while(|&c| is_b64(c)).collect();
            if b64.len() == 44 && b64.ends_with('=') {
                result = Some(format!("sha256-{b64}"));
            }
        }
        rest = &rest[pos + 4..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINS: &str = "\
{\n  a = \"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"; # PREFETCH:llvm-mos-source\n  b = \"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"; # PREFETCH:check-vendor\n}\n";

    #[test]
    fn finds_unique_marker() {
        assert_eq!(find_marker_line(PINS, "PREFETCH:check-vendor").unwrap(), 2);
        assert!(find_marker_line(PINS, "PREFETCH:nope").is_err());
    }

    #[test]
    fn replaces_only_target_line() {
        let got = "sha256-dGhpcyBpcyBhIHRlc3QgaGFzaCwgNDQgY2hhcnMhIQ=";
        let out = replace_on_line(PINS, 2, PLACEHOLDER, got).unwrap();
        assert_eq!(out.matches(got).count(), 1);
        assert_eq!(out.matches(PLACEHOLDER).count(), 1); // line 1 untouched
        assert!(out.lines().nth(2).unwrap().contains(got));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn extracts_last_got_hash() {
        let log = "error: hash mismatch in fixed-output derivation '/nix/store/x.drv':\n\
                   \x20        specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n\
                   \x20           got:    sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=\n";
        assert_eq!(
            extract_got_hash(log).unwrap(),
            "sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
        assert!(extract_got_hash("some unrelated failure").is_none());
    }
}
