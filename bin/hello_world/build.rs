use std::{
    env,
    path::{Path, PathBuf},
};

type Result<T, E = Box<dyn core::error::Error>> = core::result::Result<T, E>;

/// This project's linker script, if it has one. Optional: without it the SDK's
/// own layout applies, which carries a program until it needs a symbol at a
/// fixed address. See `memory.x.example` beside this file.
const MEMORY_SCRIPT: &str = "memory.x";

/// cargo subcommands that have a cross-compiling counterpart, and the xtask verb
/// that provides it. Anything absent here — `test`, `clippy`, `doc`, … — is a
/// legitimate host command with no cross-compiled equivalent, and must stay
/// silent.
const XTASK_COUNTERPARTS: &[(&str, &str)] = &[
    ("build", "build"),
    ("check", "check"),
    ("run", "run"),
    ("rustc", "asm"),
];

/// Warn when a host build came from a command that has a cross-compiling
/// counterpart, naming what was actually typed.
///
/// The suggestion is the `cargo xtask <verb>` form rather than the shorter
/// `cargo x<verb>`: the short form is a shim the `nix develop` shell puts on
/// PATH, so outside that shell it does not exist and cargo answers "no such
/// subcommand". `cargo xtask` is a plain alias that always resolves, and it
/// reports the missing toolchain itself. The alias resolves `--manifest-path`
/// against the cwd, so the message names the directory to run it from.
///
/// This is a warning rather than a prompt because a build script has no
/// terminal: its stdin reads EOF immediately and its stderr is captured to
/// `target/*/build/*/stderr` rather than shown. `cargo::warning=` is the only
/// channel cargo surfaces on a successful build.
///
/// Cargo exposes no "what did the user type" variable, so we read the parent
/// process's command line — the build script is spawned directly by cargo. If
/// that lookup fails we stay silent rather than guess.
///
/// Naming the verb requires this script to run on the build being warned about.
/// Cargo otherwise caches build script output and replays it, so the message
/// would keep naming `cargo rustc` long after the user moved on to `cargo run`.
/// Hence the rerun-if-changed on a path that does not exist, emitted by the
/// caller: cargo re-runs the script whenever a declared path is missing. That
/// costs a recompile of this crate per build — paid only on host builds, which
/// are the mistake this warns about, because build script output is cached per
/// target and the caller emits the directive on the host path alone. Builds for
/// a Commodore machine keep their caching.
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
    let Some((_, verb)) = XTASK_COUNTERPARTS.iter().find(|(c, _)| *c == subcommand) else {
        return; // no counterpart (e.g. `cargo test`) — nothing to suggest
    };
    // Two lines, and no more: cargo prefixes each directive with
    // `warning: <package>:` and reprints the whole block when the build fails, so
    // the user sees double. What the fix is, and what to type. The `nix develop`
    // reminder is left out on purpose — `cargo xtask` says so itself when the
    // toolchain is missing, which is why it is the form suggested here.
    let from = match xtask_root() {
        Some(root) => format!("   (from {})", root.display()),
        None => String::new(),
    };
    println!("cargo::warning=`cargo {subcommand}` builds for the host, not a MOS CPU-based machine.");
    println!("cargo::warning=use:  cargo xtask {verb} --target c64|mega65{from}");
}

/// The rust-mos-env root — the nearest ancestor holding the `xtask` crate the
/// `cargo xtask` alias names. `None` if this project has been moved out of the
/// environment, in which case the warning simply omits the directory.
fn xtask_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").ok()?);
    manifest_dir
        .ancestors()
        .find(|dir| dir.join("xtask/Cargo.toml").is_file())
        .map(Path::to_path_buf)
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
    // Watch the package directory rather than naming build.rs and memory.x:
    // cargo re-runs a build script on every build when a declared path is
    // missing, so naming an absent memory.x would cost a rebuild every time.
    // Watching the directory also notices a memory.x added later.
    println!("cargo:rerun-if-changed=.");

    // Only the bare-metal mos targets get a linker script. Host builds (the
    // xtask helper, `cargo test`) must not have `-Tmemory.x` forced on them —
    // it isn't even a valid flag for the host linker.
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("mos") {
        // A path that never exists, so cargo re-runs this script on every host
        // build and the warning below can name the command actually typed rather
        // than replay a cached one. Emitted only here: build script output is
        // cached per target, so a Commodore build is unaffected.
        println!("cargo:rerun-if-changed=.rerun-every-host-build");
        warn_if_host_build_has_xtask_counterpart();
        return Ok(());
    }

    // Cargo runs a build script with the cwd set to its package root, so the
    // link search path is this project's own directory.
    //
    // Nothing to do when the project has no memory.x: the SDK's own linker
    // script already lays out a working program, which is why hello_world ships
    // without one. A project that must pin a symbol at a fixed address — a
    // character set at $2000, sprite data, music — supplies memory.x and the
    // `-T` below hands it to the linker.
    let project = env::current_dir()?;
    if project.join(MEMORY_SCRIPT).is_file() {
        println!("cargo:rustc-link-search={}", project.display());
        println!("cargo:rustc-link-arg-bins=-T{MEMORY_SCRIPT}");
    }

    Ok(())
}
