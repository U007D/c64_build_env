use std::{env, fs, path::PathBuf};

type Result<T, E = Box<dyn core::error::Error>> = core::result::Result<T, E>;

/// The linker script is per-machine: the C64 pins `.font`/`.music` to fixed
/// addresses that are invalid on the MEGA65 (whose PRG loads at $2001, inside
/// the C64's `.font` placement). Pick the right one from the target vendor —
/// `"c64"` / `"mega65"` in targets/mos-*-none.json — and stage it under the
/// single name the link arg names, so the `-Tmemory.x` below never has to vary.
fn memory_script(vendor: &str) -> Result<&'static str> {
    match vendor {
        "c64" => Ok("memory-c64.x"),
        "mega65" => Ok("memory-mega65.x"),
        other => Err(format!(
            "no linker script for mos target vendor '{other}'; add memory-{other}.x and a match arm here"
        )
        .into()),
    }
}

/// cargo subcommands that have an `x` counterpart, and the counterpart's name.
/// Anything absent here — `test`, `clippy`, `doc`, … — is a legitimate host
/// command with no cross-compiled equivalent, and must stay silent.
const XTASK_COUNTERPARTS: &[(&str, &str)] = &[
    ("build", "xbuild"),
    ("check", "xcheck"),
    ("run", "xrun"),
    ("rustc", "xasm"),
];

/// Warn when a host build came from a command that has a cross-compiling
/// counterpart, naming what was actually typed.
///
/// This is a warning rather than a prompt because a build script has no
/// terminal: its stdin reads EOF immediately and its stderr is captured to
/// `target/*/build/*/stderr` rather than shown. `cargo::warning=` is the only
/// channel cargo surfaces on a successful build.
///
/// Cargo exposes no "what did the user type" variable, so we read the parent
/// process's command line — the build script is spawned directly by cargo. If
/// that lookup fails we stay silent rather than guess.
fn warn_if_host_build_has_xtask_counterpart() {
    let Some(cmdline) = parent_command_line() else {
        return;
    };
    // First non-flag token after the cargo binary is the subcommand.
    let Some(subcommand) = cmdline
        .split_whitespace()
        .skip(1)
        .find(|t| !t.starts_with('-'))
    else {
        return;
    };
    let Some((_, x)) = XTASK_COUNTERPARTS.iter().find(|(c, _)| *c == subcommand) else {
        return; // no counterpart (e.g. `cargo test`) — nothing to suggest
    };
    println!(
        "cargo::warning=`cargo {subcommand}` targets the host ({}). For the C64 use \
         `cargo {x}`; for the MEGA65, `cargo {x}_mega65`.",
        env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
}

/// The parent process's full command line, via `ps` (no dependencies, and
/// `/proc` does not exist on macOS).
fn parent_command_line() -> Option<String> {
    let field = |flag: &str, pid: &str| -> Option<String> {
        let out = std::process::Command::new("ps")
            .args(["-o", flag, "-p", pid])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let ppid = field("ppid=", &std::process::id().to_string())?;
    field("args=", &ppid)
}

fn main() -> Result<()> {
    // Rerun when any input changes, regardless of which one today's target picks.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory-c64.x");
    println!("cargo:rerun-if-changed=memory-mega65.x");

    // Only the bare-metal mos targets get a linker script. Host builds (the
    // xtask helper, `cargo test`) must not have `-Tmemory.x` forced on them —
    // it isn't even a valid flag for the host linker.
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("mos") {
        warn_if_host_build_has_xtask_counterpart();
        return Ok(());
    }

    let vendor = env::var("CARGO_CFG_TARGET_VENDOR")?;
    let script = memory_script(&vendor)?;

    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::copy(script, out.join("memory.x"))?;

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg-bins=-Tmemory.x");

    Ok(())
}
