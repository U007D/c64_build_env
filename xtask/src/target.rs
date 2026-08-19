//! The machine a cross-compile command targets, and the `--target <name>`
//! parsing shared by build / check / run / asm.
//!
//! `--target` is **required** on those commands: the two machines produce
//! incompatible binaries (different CPU, different linker, different emulator),
//! so there is no safe default to fall back to. Omitting it — or naming an
//! unknown machine — prints usage listing the valid names.
//!
//! Note this is xtask's *own* `--target <name>` (`c64`, `mega65`), which is
//! translated into cargo's `--target <triple>` (`mos-c64-none`,
//! `mos-mega65-none`). Users never type the triple.

use std::process::ExitCode;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Target {
    C64,
    Mega65,
}

impl Target {
    /// Every selectable machine, in the order `usage` lists them.
    pub(crate) const ALL: &'static [Target] = &[Target::C64, Target::Mega65];

    /// The name the user types after `--target`.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::C64 => "c64",
            Self::Mega65 => "mega65",
        }
    }

    /// The rustc target triple. `mos-c64-none` ships with rust-mos;
    /// `mos-mega65-none` is this repo's `targets/mos-mega65-none.json`, which the
    /// dev shell puts on `RUST_TARGET_PATH`.
    pub(crate) const fn triple(self) -> &'static str {
        match self {
            Self::C64 => "mos-c64-none",
            Self::Mega65 => "mos-mega65-none",
        }
    }

    /// One-line description for `usage`.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::C64 => "Commodore 64 — 6502, runs in VICE (x64sc)",
            Self::Mega65 => "MEGA65 — 45GS02, runs in Xemu (xmega65)",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.name() == s)
    }

    /// Cross-compile flags passed explicitly on every game build, so xtask does
    /// not depend on an ancestor `.cargo/config.toml` default — cargo discovers
    /// config by cwd, so such a default would also leak onto this host tool and
    /// break `cargo xtask`. `-Zbuild-std` needs the forked nightly cargo, so
    /// these commands must run inside `nix develop`.
    pub(crate) fn cargo_flags(self) -> Vec<String> {
        let mut flags = vec![
            "--target".to_string(),
            self.triple().to_string(),
            "-Zbuild-std=core,alloc".to_string(),
            "-Zbuild-std-features=panic_immediate_abort".to_string(),
        ];
        flags.extend(self.workarounds());
        flags
    }

    /// Per-machine workarounds for toolchain bugs, as `--config` overrides.
    ///
    /// The MEGA65 entry: cargo turns overflow checks on in the dev profile, and
    /// the checked-arithmetic branches they insert defeat the 45GS02 register
    /// allocator — rustc dies with a SIGSEGV inside LLVM's RAGreedy compiling
    /// core and compiler_builtins for mos-mega65-none, whatever the opt-level
    /// and LTO setting. Scoping the override to dependencies leaves your own
    /// code checked, and passing it here rather than in the workspace
    /// `[profile]` leaves the C64 and host builds checked too — a `[profile]`
    /// cannot be conditioned on the target, and only the 45GS02 has this bug.
    ///
    /// Each workaround has a probe in check.nix that fails the flake check once
    /// the bug is fixed upstream, naming what to delete.
    fn workarounds(self) -> Vec<String> {
        match self {
            Self::C64 => vec![],
            Self::Mega65 => vec![
                "--config".to_string(),
                r#"profile.dev.package."*".overflow-checks=false"#.to_string(),
            ],
        }
    }
}

/// Pull `--target <name>` / `--target=<name>` out of `args`, returning the
/// machine and the remaining args (forwarded verbatim to cargo).
///
/// The flag is consumed rather than forwarded: cargo takes `--target <triple>`,
/// which `Target::cargo_flags` supplies. Passing both would conflict.
pub(crate) fn split(args: &[String]) -> Result<(Target, Vec<String>), String> {
    let mut target = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = if let Some(v) = a.strip_prefix("--target=") {
            Some(v.to_string())
        } else if a == "--target" {
            match it.next() {
                Some(v) => Some(v.clone()),
                None => return Err("--target requires a value".to_string()),
            }
        } else {
            rest.push(a.clone());
            None
        };
        if let Some(v) = value {
            match Target::parse(&v) {
                Some(t) => target = Some(t),
                None => return Err(format!("unknown target: {v}")),
            }
        }
    }
    target.ok_or_else(|| "missing required --target".to_string())
        .map(|t| (t, rest))
}

/// `split`, but on failure print the reason plus usage and hand back the exit
/// code — the shape every cross-compile command's `run` wants.
pub(crate) fn split_or_usage(
    args: &[String],
    command: &str,
) -> Result<(Target, Vec<String>), ExitCode> {
    // Every cross-compile command funnels through here, so it is the one place
    // to check the dev shell. A missing shell is fatal (nothing can work); a
    // stale one only warns.
    if let Err(e) = crate::devshell::check() {
        eprintln!("ERROR [{command}]: {e}");
        return Err(ExitCode::FAILURE);
    }
    match split(args) {
        Ok(v) => Ok(v),
        Err(e) => {
            eprintln!("ERROR [{command}]: {e}");
            eprintln!();
            usage(command);
            Err(ExitCode::FAILURE)
        }
    }
}

/// Print the `--target` usage for `command`, listing every valid machine.
pub(crate) fn usage(command: &str) {
    eprintln!("usage: cargo x{command} --target <TARGET> [CARGO ARGS…]");
    eprintln!();
    eprintln!("targets:");
    let width = Target::ALL.iter().map(|t| t.name().len()).max().unwrap_or(0);
    for t in Target::ALL {
        eprintln!("  {:<width$}  {}", t.name(), t.description());
    }
    eprintln!();
    eprintln!("shorthand aliases (from .cargo/config.toml, repo root only):");
    for t in Target::ALL {
        eprintln!("  cargo x{command}_{}", t.name());
    }
    eprintln!("  cargo x{command}  — same as cargo x{command}_c64");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_target_names() {
        assert_eq!(Target::parse("c64"), Some(Target::C64));
        assert_eq!(Target::parse("mega65"), Some(Target::Mega65));
        assert_eq!(Target::parse("C64"), None); // exact match only
        assert_eq!(Target::parse("vic20"), None);
    }

    #[test]
    fn splits_target_from_cargo_args() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // Space-separated form, flag consumed, rest forwarded in order.
        let (t, rest) = split(&args(&["--target", "mega65", "--release", "-v"])).unwrap();
        assert_eq!(t, Target::Mega65);
        assert_eq!(rest, args(&["--release", "-v"]));

        // `=` form, and the flag may appear after other args.
        let (t, rest) = split(&args(&["--release", "--target=c64"])).unwrap();
        assert_eq!(t, Target::C64);
        assert_eq!(rest, args(&["--release"]));

        // Missing, unknown, and dangling are all errors.
        assert!(split(&args(&["--release"])).is_err());
        assert!(split(&args(&["--target", "vic20"])).is_err());
        assert!(split(&args(&["--target"])).is_err());
    }

    #[test]
    fn cargo_flags_name_the_triple() {
        assert!(Target::C64.cargo_flags().contains(&"mos-c64-none".to_string()));
        assert!(Target::Mega65.cargo_flags().contains(&"mos-mega65-none".to_string()));
    }

    #[test]
    fn only_mega65_disables_dependency_overflow_checks() {
        let has_override = |t: Target| {
            t.cargo_flags()
                .iter()
                .any(|f| f.contains("overflow-checks=false"))
        };
        assert!(has_override(Target::Mega65));
        // The C64 backend has no such bug, so its builds keep the checks.
        assert!(!has_override(Target::C64));
    }
}
