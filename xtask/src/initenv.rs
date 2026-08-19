//! `initenv` — one idempotent command that gives you a working build
//! environment: installs plain upstream Nix if missing (pinned, checksum-verified
//! installer), generates `flake.lock` if absent, fills the `PREFETCH:` source
//! hashes in toolchain/pins.nix if still placeholders, then runs the flake's full
//! check (`nix flake check`). `--build` builds just the rust-mos toolchain and
//! skips the checks. Every step skips itself if already done. It also enables the
//! flakes feature so a bare `nix develop` works.

use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::prefetch::{pin_all, PLACEHOLDER};
use crate::{build_toolchain, find_nix, repo_root, run_nix_inherit, sha256_file};

pub const HELP: &str = "\
cargo xtask initenv [--build] [--from-source]

Install Nix if needed, then build + check the flake. The default runs
`nix flake check` (proves the toolchain compiles a C64 program offline).
  --build        build only the rust-mos toolchain, skipping the checks
  --from-source  disable binary caches — build the whole closure (incl.
                 nixpkgs) from source
";

/// Pinned `NixOS/nix-installer` release. This tag also pins the upstream Nix
/// version it installs; bumping it is a deliberate action that requires
/// refreshing `INSTALLER_SHA256` from the release's `SHA256SUMS` asset.
const NIX_INSTALLER_VERSION: &str = "2.35.1";

/// sha256 of each pinned installer binary, from the release's `SHA256SUMS`.
/// Keyed by the installer's arch triple (which is also the asset suffix).
const INSTALLER_SHA256: &[(&str, &str)] = &[
    (
        "aarch64-darwin",
        "82723616373d0c3f0d07b892f5f5c023da825b8969a2351c7055926d0bcf5553",
    ),
    (
        "aarch64-linux",
        "7e6e2f753144d7f19b16a9fce4b354cb0f46d1d47e6908bfb9186c89e0e0e649",
    ),
    (
        "x86_64-linux",
        "3b49a0b91820accb76e3d9ff7ed64fc430121b9fafb3869b0d549721fbeb4c85",
    ),
];

struct InitFlags {
    build_only: bool,
    from_source: bool,
}

fn parse_init_flags(args: &[String]) -> Result<InitFlags, String> {
    let mut build_only = false;
    let mut from_source = false;
    for a in args {
        match a.as_str() {
            "--build" => build_only = true,
            "--from-source" => from_source = true,
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(InitFlags {
        build_only,
        from_source,
    })
}

/// What kind of platform we're on, from the point of view of *installing* Nix.
#[derive(Debug, PartialEq)]
enum Platform {
    /// A pinned installer binary exists for this arch triple (the asset suffix).
    Installer(&'static str),
    /// Intel macOS: no pinned binary is published — use the official script.
    IntelMac,
    /// Windows: Nix can't install natively — needs WSL2.
    Windows,
    Unsupported,
}

fn resolve_platform(os: &str, arch: &str) -> Platform {
    match (os, arch) {
        ("macos", "aarch64") => Platform::Installer("aarch64-darwin"),
        ("linux", "x86_64") => Platform::Installer("x86_64-linux"),
        ("linux", "aarch64") => Platform::Installer("aarch64-linux"),
        ("macos", "x86_64") => Platform::IntelMac,
        ("windows", _) => Platform::Windows,
        _ => Platform::Unsupported,
    }
}

pub fn run(args: &[String]) -> ExitCode {
    match init_buildenv_inner(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [initenv]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn init_buildenv_inner(args: &[String]) -> Result<(), String> {
    let flags = parse_init_flags(args)?;
    let root = repo_root();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // 1. Ensure Nix. If it's already present we use it regardless of platform
    //    (e.g. an Intel Mac that installed Nix by hand still builds fine).
    let nix = match find_nix() {
        Some(n) => {
            println!("Using existing Nix: {}", n.display());
            n
        }
        None => match resolve_platform(os, arch) {
            Platform::Installer(installer_arch) => install_nix(installer_arch, &root)?,
            Platform::IntelMac => return Err(intel_mac_message()),
            Platform::Windows => return Err(windows_message()),
            Platform::Unsupported => {
                return Err(format!("unsupported platform: {os}/{arch}"));
            }
        },
    };

    // 1b. Make sure the `nix develop` we recommend later works. initenv threads the
    //     flakes flag onto its own calls, but a bare `nix develop` the user types
    //     does not — so offer to enable flakes (asking first, since it touches the
    //     user's global Nix config). Done up front so it isn't waiting after the build.
    ensure_flakes_enabled(&nix, &root);

    // 2. Ensure flake.lock (deterministic — flake.nix pins nixpkgs to an exact rev).
    let lock_generated = ensure_lock(&nix, &root)?;

    // 3. Ensure the source hashes are filled.
    let pins_path = root.join("toolchain/pins.nix");
    if !pins_path.is_file() {
        return Err(format!("{} not found", pins_path.display()));
    }
    let hashes_filled = ensure_hashes(&nix, &root, &pins_path)?;

    // 4. Build. Default is the fuller `nix flake check` (pit of success: it proves the
    //    toolchain can actually compile a C64 program offline). `--build` opts into just
    //    building the toolchain.
    // --from-source disables all binary caches, so anything not already in the local
    // /nix/store is built from source — including nixpkgs dependencies. That is a lot;
    // warn so it's not a surprise.
    let extra: &[&str] = if flags.from_source {
        eprintln!(
            "--from-source: disabling binary caches (--no-substitute). Everything not already \
             in your /nix/store — including nixpkgs dependencies (clang, LLVM, …) — is built \
             from source. This can take a very long time."
        );
        &["--no-substitute"]
    } else {
        &[]
    };
    if flags.build_only {
        println!("Building the rust-mos toolchain (first build is LLVM-scale: very roughly 2-4 h)…");
        let out = build_toolchain(&nix, &root, extra)?;
        println!();
        println!("Build environment ready.");
        if !out.is_empty() {
            println!("  rust-mos: {out}");
        }
    } else {
        println!("Building + checking the flake (`nix flake check`; first build is LLVM-scale: very roughly 2-4 h)…");
        let mut args = vec!["flake", "check"];
        args.extend_from_slice(extra);
        if !run_nix_inherit(&nix, &root, &args)? {
            return Err("`nix flake check` failed".into());
        }
        println!();
        println!("Build environment ready (full flake check passed).");
    }

    // 4b. Emulators. `cargo xrun_mega65` needs Xemu's `xmega65`, which this flake
    //     builds from source because neither nixpkgs nor Homebrew packages it
    //     (both ship an unrelated Xbox emulator under the name `xemu`). Neither
    //     `nix build .#rust-mos` nor `nix flake check` reaches it, so build it
    //     here — otherwise the cost lands on the user's first `nix develop`.
    //     Non-fatal: a failure here still leaves a working C64 toolchain, and the
    //     C64 path (VICE) is unaffected.
    println!();
    println!("Building Xemu (xmega65) for the MEGA65 target…");
    let mut xemu_args = vec!["build", ".#xemu"];
    xemu_args.extend_from_slice(extra);
    match run_nix_inherit(&nix, &root, &xemu_args) {
        Ok(true) => println!("Xemu ready (`cargo xrun_mega65`)."),
        Ok(false) | Err(_) => {
            eprintln!("WARNING: `nix build .#xemu` failed — the MEGA65 emulator is unavailable.");
            eprintln!("         C64 builds and `cargo xrun` (VICE) are unaffected.");
        }
    }

    // 4c. The C65 ROM xmega65 boots. Also non-fatal, and skipped when already
    //     installed — the fetch is a ~268 MB installer.
    if let Err(e) = install_mega65_rom(&nix, &root, extra) {
        eprintln!("WARNING: MEGA65 ROM not installed: {e}");
        eprintln!("         xmega65 still runs, but boots its stub ROM. See README (\"MEGA65 ROM\").");
    }

    // 5. If we generated anything, remind the user to commit it.
    if lock_generated || hashes_filled {
        println!();
        println!("NOTE: flake.lock / toolchain/pins.nix were generated on this run. Commit the repo so");
        println!("      the next init finds them pinned:  git init && git add -A && git commit");
    }

    // 6. Next steps.
    println!();
    println!("Next:");
    println!("  restart your shell (the nix profile isn't on this process's PATH yet)");
    println!("  nix develop        # rust-mos rustc + its cargo + SDK on PATH");
    println!("  cd bin/hello_world && cargo xrun   # build the example and launch VICE");
    Ok(())
}

/// Xemu's preferences directory for the MEGA65 target — where it looks for
/// `MEGA65.ROM` (`targets/mega65/rom.c` loads `"@MEGA65.ROM"`, `@` meaning the
/// pref dir). This is SDL_GetPrefPath("xemu-lgb", "mega65"), whose layout is
/// platform-defined; we reproduce the two we support.
fn xemu_pref_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let home = PathBuf::from(home);
    Ok(if cfg!(target_os = "macos") {
        home.join("Library/Application Support/xemu-lgb/mega65")
    } else {
        home.join(".local/share/xemu-lgb/mega65")
    })
}

/// Build `.#mega65-rom` and copy it into Xemu's preferences directory.
///
/// The ROM is Cloanto copyright — free to download for your own use, not
/// redistributable. It is fetched to your local /nix/store and copied to your
/// own pref dir; it never enters this repo or any binary cache. See
/// toolchain/mega65-rom.nix.
fn install_mega65_rom(nix: &Path, root: &Path, extra: &[&str]) -> Result<(), String> {
    let dest_dir = xemu_pref_dir()?;
    let dest = dest_dir.join("MEGA65.ROM");
    if dest.exists() {
        match rom_version(&dest) {
            Some(v) => println!("MEGA65 ROM already installed (version {v}): {}", dest.display()),
            None => println!("MEGA65 ROM already installed: {}", dest.display()),
        }
        report_rom_capability(&dest);
        return Ok(());
    }

    println!();
    println!("Fetching the MEGA65 (C65 910828) ROM from Cloanto's free C64 Forever installer…");
    println!("  ~268 MB download; the ROM is for your own use and is not redistributed.");
    // The derivation is meta.license = unfree (the ROM is Cloanto's, free to
    // download but not to redistribute), so nixpkgs refuses to build it unless
    // the user opts in. Opt in explicitly here — downloading it for your own use
    // is exactly what Cloanto permits and what Xemu's docs recommend. That in
    // turn requires --impure, since the env var is an impurity.
    let mut args = vec![
        "build",
        ".#mega65-rom",
        "--print-out-paths",
        "--no-link",
        "--impure",
    ];
    args.extend_from_slice(extra);
    let out = crate::nix_command(nix, root, &args)
        .env("NIXPKGS_ALLOW_UNFREE", "1")
        .output()
        .map_err(|e| format!("failed to spawn `nix`: {e}"))?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!(
            "`nix build .#mega65-rom` failed (has Cloanto moved the installer? see README):\n{}",
            combined.trim()
        ));
    }
    let store_rom = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).join("MEGA65.ROM");
    if !store_rom.exists() {
        return Err(format!("built, but {} is missing", store_rom.display()));
    }

    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("creating {}: {e}", dest_dir.display()))?;
    std::fs::copy(&store_rom, &dest).map_err(|e| format!("copying to {}: {e}", dest.display()))?;
    // The store copy is read-only; make the installed one writable so Xemu's UI
    // can replace it later without a permission error.
    if let Ok(meta) = std::fs::metadata(&dest) {
        let mut perms = meta.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o644);
        }
        #[cfg(not(unix))]
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(&dest, perms);
    }
    println!("MEGA65 ROM installed: {}", dest.display());
    report_rom_capability(&dest);
    Ok(())
}

/// A closed-ROM stamps `V` + a 6-digit version at offset $16 — the same probe
/// Xemu uses (`targets/mega65/rom.c` checks 0x56 at 0x16). Returns e.g. 910814
/// for the stock C65 ROM, 920422 for a patched MEGA65 one.
fn rom_version(path: &Path) -> Option<u32> {
    let bytes = std::fs::read(path).ok()?;
    let tag = bytes.get(0x16..0x1d)?;
    if tag[0] != b'V' {
        return None;
    }
    std::str::from_utf8(&tag[1..]).ok()?.parse().ok()
}

/// Tell the user whether the installed ROM can actually run MEGA65 programs.
///
/// The stock C65 ROM (91xxxx) boots Xemu but is a plain C65: llvm-mos's MEGA65
/// startup assumes a MEGA65 memory environment, so programs crash immediately.
/// Running code needs the MEGA65 project's enhanced closed-ROM (92xxxx+), which
/// is owner-licensed and cannot be installed automatically.
fn report_rom_capability(path: &Path) {
    match rom_version(path) {
        Some(v) if v >= 920000 => {
            println!("  MEGA65 closed-ROM v{v} — `cargo xrun_mega65` will run programs.");
        }
        other => {
            let what = match other {
                Some(v) => format!("the stock C65 ROM (v{v})"),
                None => "an unrecognised ROM".to_string(),
            };
            println!();
            println!("NOTE: this is {what}. Xemu starts, but your MEGA65 programs may crash.");
            println!("      To run them, patch this ROM to a MEGA65 ROM (v920422+) and save it over");
            println!("      {}", path.display());
            println!("      How:   https://retrocombs.com/patch-c65-rom");
            println!("      Tools: https://files.mega65.org  (M65Connect + the .BDF diff)");
        }
    }
}

fn intel_mac_message() -> String {
    "no pinned Nix installer binary is published for Intel macOS (x86_64-darwin).\n\
     Install Nix with the official script, then re-run `cargo xtask initenv`:\n  \
     curl -L https://nixos.org/nix/install | sh -s -- --daemon"
        .to_string()
}

fn windows_message() -> String {
    "Nix cannot install on native Windows. Install WSL2 (`wsl --install`), then run\n\
     `cargo xtask initenv` inside the WSL2 Linux shell."
        .to_string()
}

/// Download the pinned installer, verify its checksum, run it, and return the
/// absolute path to the installed `nix` (which is NOT on this process's PATH).
fn install_nix(installer_arch: &str, root: &Path) -> Result<PathBuf, String> {
    let want = INSTALLER_SHA256
        .iter()
        .find(|(a, _)| *a == installer_arch)
        .map(|(_, h)| *h)
        .ok_or_else(|| format!("no pinned installer checksum for {installer_arch}"))?;

    let url = format!(
        "https://github.com/NixOS/nix-installer/releases/download/{NIX_INSTALLER_VERSION}/nix-installer-{installer_arch}"
    );
    let dst = root.join("target").join(format!("nix-installer-{installer_arch}"));
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    println!("Downloading pinned Nix installer {NIX_INSTALLER_VERSION} ({installer_arch})…");
    let status = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-sSfL", &url, "-o"])
        .arg(&dst)
        .status()
        .map_err(|e| format!("failed to spawn curl: {e} (is curl installed?)"))?;
    if !status.success() {
        return Err("downloading the Nix installer failed".into());
    }

    let got = sha256_file(&dst)?;
    if !got.eq_ignore_ascii_case(want) {
        let _ = fs::remove_file(&dst);
        return Err(format!(
            "installer checksum mismatch for {installer_arch}: got {got}, want {want}"
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod {}: {e}", dst.display()))?;
    }

    println!("Installing plain upstream Nix (multi-user daemon; sudo may prompt for your password)…");
    let status = Command::new(&dst)
        .args([
            "install",
            "--no-confirm",
            // Enable flakes in the system nix.conf at install time, so plain
            // `nix` commands (e.g. `nix develop`) work without extra flags.
            "--extra-conf",
            "extra-experimental-features = nix-command flakes",
        ])
        .status()
        .map_err(|e| format!("failed to run the Nix installer: {e}"))?;
    if !status.success() {
        return Err("the Nix installer failed".into());
    }

    let nix = PathBuf::from("/nix/var/nix/profiles/default/bin/nix");
    if !nix.exists() {
        return Err(format!(
            "installer finished but {} was not found",
            nix.display()
        ));
    }
    println!("Nix installed. Restart your shell later for interactive `nix` use.");
    Ok(nix)
}

/// Generate `flake.lock` if it's missing. Returns whether it was generated.
fn ensure_lock(nix: &Path, root: &Path) -> Result<bool, String> {
    if root.join("flake.lock").is_file() {
        println!("flake.lock present.");
        return Ok(false);
    }
    println!("Generating flake.lock (pinning nixpkgs)…");
    if !run_nix_inherit(nix, root, &["flake", "lock"])? {
        return Err("`nix flake lock` failed".into());
    }
    Ok(true)
}

/// Fill any remaining `PREFETCH:` placeholders. Returns whether anything changed.
fn ensure_hashes(nix: &Path, root: &Path, pins_path: &Path) -> Result<bool, String> {
    let contents = fs::read_to_string(pins_path).map_err(|e| format!("reading pins.nix: {e}"))?;
    if !contents.contains(PLACEHOLDER) {
        println!("Source hashes already pinned.");
        return Ok(false);
    }
    println!("Filling source hashes (first run only; several GB of downloads)…");
    pin_all(nix, root, pins_path)
}

/// Ensure plain `nix` commands the user runs themselves (like `nix develop`) can
/// use flakes. If flakes is already active, do nothing. Otherwise this modifies
/// the user's global `~/.config/nix/nix.conf`, so it always **asks first**,
/// showing the exact file and line; declined or non-interactive → it prints the
/// manual command and changes nothing.
fn ensure_flakes_enabled(nix: &Path, root: &Path) {
    let active = flakes_enabled(nix, root);
    if active == Some(true) {
        return;
    }
    let conf = match user_nix_conf() {
        Some(c) => c,
        None => return,
    };
    let existing = fs::read_to_string(&conf).unwrap_or_default();

    // The line is already in the file — don't touch it. If it's there but flakes
    // still isn't active, the machine is ignoring user config; point at the fix.
    if conf_enables_flakes(&existing) {
        if active == Some(false) {
            print_untrusted_note();
        }
        return;
    }

    println!();
    println!("`nix develop` needs the flakes feature, which isn't enabled on this machine.");
    if conf.is_file() {
        println!("initenv can add one line to your existing Nix config (it won't touch anything else):");
    } else {
        println!("initenv can create your Nix config with one line:");
    }
    println!("  file: {}", conf.display());
    println!("  line: extra-experimental-features = nix-command flakes");

    if !std::io::stdin().is_terminal() {
        println!("(non-interactive — leaving your config untouched). To enable it yourself:");
        print_flakes_manual(&conf);
        return;
    }
    if !prompt_yes_no("Add it now? [y/N] ") {
        println!("Left your config untouched. To enable flakes yourself:");
        print_flakes_manual(&conf);
        return;
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str("extra-experimental-features = nix-command flakes\n");
    if let Some(dir) = conf.parent() {
        let _ = fs::create_dir_all(dir);
    }
    match fs::write(&conf, updated) {
        Ok(()) => {
            println!("Enabled flakes in {}.", conf.display());
            // Multi-user hosts can ignore an untrusted user's config; if so, say so.
            if flakes_enabled(nix, root) == Some(false) {
                print_untrusted_note();
            }
        }
        Err(e) => {
            println!("Could not write {}: {e}", conf.display());
            print_flakes_manual(&conf);
        }
    }
}

/// Print the copy-paste commands to enable flakes in `conf` by hand.
fn print_flakes_manual(conf: &Path) {
    if let Some(dir) = conf.parent() {
        println!("  mkdir -p {}", dir.display());
    }
    println!(
        "  echo 'extra-experimental-features = nix-command flakes' >> {}",
        conf.display()
    );
}

/// Print the system-level (sudo) fallback for hosts that ignore user config.
fn print_untrusted_note() {
    println!("NOTE: flakes still isn't active for plain `nix` (this host may ignore an");
    println!("      untrusted user's config). Enable it system-wide (needs sudo):");
    println!("        echo 'extra-experimental-features = flakes' | sudo tee -a /etc/nix/nix.custom.conf");
}

/// Prompt on stdout and read a yes/no answer; anything not starting with y/Y is no.
fn prompt_yes_no(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().chars().next(), Some('y' | 'Y'))
}

/// Whether `flakes` is persistently enabled for plain `nix`. Probed with only
/// `nix-command` forced on (never `flakes`), so the result reflects the config,
/// not our own flag. `None` if the probe couldn't run.
fn flakes_enabled(nix: &Path, root: &Path) -> Option<bool> {
    let out = Command::new(nix)
        .args([
            "--extra-experimental-features",
            "nix-command",
            "config",
            "show",
            "experimental-features",
        ])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .any(|w| w == "flakes"),
    )
}

/// True if the nix.conf text already turns flakes on via an
/// `(extra-)experimental-features` line.
fn conf_enables_flakes(contents: &str) -> bool {
    contents.lines().any(|l| {
        let l = l.trim();
        (l.starts_with("experimental-features") || l.starts_with("extra-experimental-features"))
            && l.contains("flakes")
    })
}

fn user_nix_conf() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/nix/nix.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_platforms() {
        assert_eq!(
            resolve_platform("macos", "aarch64"),
            Platform::Installer("aarch64-darwin")
        );
        assert_eq!(
            resolve_platform("linux", "x86_64"),
            Platform::Installer("x86_64-linux")
        );
        assert_eq!(
            resolve_platform("linux", "aarch64"),
            Platform::Installer("aarch64-linux")
        );
        assert_eq!(resolve_platform("macos", "x86_64"), Platform::IntelMac);
        assert_eq!(resolve_platform("windows", "x86_64"), Platform::Windows);
        assert_eq!(resolve_platform("freebsd", "x86_64"), Platform::Unsupported);
    }

    #[test]
    fn every_installer_arch_has_a_checksum() {
        for (os, arch) in [("macos", "aarch64"), ("linux", "x86_64"), ("linux", "aarch64")] {
            if let Platform::Installer(triple) = resolve_platform(os, arch) {
                assert!(
                    INSTALLER_SHA256.iter().any(|(a, _)| *a == triple),
                    "no checksum pinned for {triple}"
                );
            } else {
                panic!("{os}/{arch} should map to an installer arch");
            }
        }
    }

    #[test]
    fn parses_init_flags() {
        let d = parse_init_flags(&[]).unwrap();
        assert!(!d.build_only && !d.from_source);
        assert!(parse_init_flags(&["--build".to_string()]).unwrap().build_only);
        let fs = parse_init_flags(&["--from-source".to_string()]).unwrap();
        assert!(fs.from_source && !fs.build_only);
        let both =
            parse_init_flags(&["--build".to_string(), "--from-source".to_string()]).unwrap();
        assert!(both.build_only && both.from_source);
        assert!(parse_init_flags(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn detects_flakes_in_conf() {
        assert!(conf_enables_flakes(
            "extra-experimental-features = nix-command flakes\n"
        ));
        assert!(conf_enables_flakes("experimental-features = flakes"));
        assert!(conf_enables_flakes(
            "max-jobs = auto\n  extra-experimental-features = flakes  \n"
        ));
        assert!(!conf_enables_flakes(
            "extra-experimental-features = nix-command\n"
        ));
        assert!(!conf_enables_flakes("# flakes are nice\nmax-jobs = auto\n"));
        assert!(!conf_enables_flakes(""));
    }
}
