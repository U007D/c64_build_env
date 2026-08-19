//! `publish-toolchain-binaries` — build the toolchain outputs and push them to a Cachix cache
//! so others download instead of rebuilding. Cache-owner, one-time. Needs
//! `CACHIX_AUTH_TOKEN`; `--public-key` wires the cache into flake.nix.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{ExitCode, Stdio};

use crate::{find_nix, nix_command, repo_root};

pub const HELP: &str = "\
cargo xtask publish-toolchain-binaries <name> [--public-key <key>]

Build the toolchain and push it to a Cachix cache so others download instead of
rebuilding (needs CACHIX_AUTH_TOKEN). With --public-key, wire the cache into
flake.nix so consumers are offered it automatically.
";

/// The outputs pushed to the binary cache (what consumers download instead of
/// rebuilding). Plumbing attrs are derived from these, so this covers the chain.
/// Each is `(flake attr, --out-link path)`: building with an explicit out-link
/// puts the resulting GC root at an explanatory path under `toolchain/` instead
/// of nix's default `result`/`result-N` in the repo root. `toolchain/gcroot-*`
/// is gitignored.
const CACHE_ATTRS: [(&str, &str); 4] = [
    (".#llvm-mos", "toolchain/gcroot-llvm-mos"),
    (".#llvm-mos-sdk", "toolchain/gcroot-llvm-mos-sdk"),
    (".#rust-mos", "toolchain/gcroot-rust-mos"),
    // Xemu is GPL-2.0+, so caching it is fine and saves every consumer a build.
    (".#xemu", "toolchain/gcroot-xemu"),
    // DO NOT add `.#mega65-rom` here. It contains Cloanto's copyrighted C65 ROM,
    // which is free to download for your own use but NOT redistributable —
    // pushing it to a public cache would be redistribution. See
    // toolchain/mega65-rom.nix.
];

/// The commented cache block shipped in flake.nix, filled in by `--public-key`.
const NIXCONFIG_COMMENTED: &str = "  # nixConfig = {\n  #   extra-substituters = [ \"https://YOUR-CACHE.cachix.org\" ];\n  #   extra-trusted-public-keys = [ \"YOUR-CACHE.cachix.org-1:PASTE-PUBLIC-KEY-HERE\" ];\n  # };";

struct PublishArgs {
    cache: String,
    public_key: Option<String>,
}

fn parse_publish_args(args: &[String]) -> Result<PublishArgs, String> {
    let mut cache: Option<String> = None;
    let mut public_key: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--public-key" => {
                i += 1;
                public_key = Some(args.get(i).ok_or("--public-key needs a value")?.clone());
            }
            s if s.starts_with("--") => return Err(format!("unknown flag: {s}")),
            s if cache.is_none() => cache = Some(s.to_string()),
            s => return Err(format!("unexpected argument: {s}")),
        }
        i += 1;
    }
    Ok(PublishArgs {
        cache: cache.ok_or("missing <cache-name>")?,
        public_key,
    })
}

pub fn run(args: &[String]) -> ExitCode {
    match publish_cache_inner(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [publish-toolchain-binaries]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn publish_cache_inner(args: &[String]) -> Result<(), String> {
    let opts = parse_publish_args(args)?;
    let root = repo_root();
    let nix = find_nix().ok_or("Nix not found. Run `cargo xtask initenv` first.")?;

    if std::env::var_os("CACHIX_AUTH_TOKEN").is_none() {
        return Err(
            "CACHIX_AUTH_TOKEN is not set. Create a cache at https://app.cachix.org, then\n  \
             export CACHIX_AUTH_TOKEN=<token>\nand re-run."
                .into(),
        );
    }

    // 1. Build each output, giving it its own --out-link so the GC root lands at a
    //    named path under toolchain/, and capture the store paths (build logs stream
    //    to stderr). One build per attr is the price of a per-attr link name; the
    //    builds are cached, so re-runs are cheap.
    println!("Building the toolchain outputs to push…");
    let mut paths = String::new();
    for (attr, out_link) in CACHE_ATTRS {
        let built = nix_command(
            &nix,
            &root,
            &["build", attr, "--out-link", out_link, "--print-out-paths"],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn `nix`: {e}"))?
        .wait_with_output()
        .map_err(|e| format!("waiting on `nix`: {e}"))?;
        if !built.status.success() {
            return Err(format!("building {attr} failed"));
        }
        paths.push_str(&String::from_utf8_lossy(&built.stdout));
    }
    if paths.trim().is_empty() {
        return Err("no store paths were produced".into());
    }

    // 2. Push via `nix run nixpkgs#cachix` — no separate cachix install needed.
    println!("Pushing to Cachix cache '{}'…", opts.cache);
    let mut child = nix_command(
        &nix,
        &root,
        &["run", "nixpkgs#cachix", "--", "push", opts.cache.as_str()],
    )
    .stdin(Stdio::piped())
    .spawn()
    .map_err(|e| format!("failed to spawn cachix via `nix run`: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin handle for cachix")?
        .write_all(paths.as_bytes())
        .map_err(|e| format!("writing store paths to cachix: {e}"))?;
    if !child
        .wait()
        .map_err(|e| format!("waiting on cachix: {e}"))?
        .success()
    {
        return Err("cachix push failed".into());
    }

    // 3. Optionally wire the cache into flake.nix so consumers get it automatically.
    match opts.public_key {
        Some(key) => {
            fill_nixconfig(&root, &opts.cache, &key)?;
            println!();
            println!("flake.nix now points at '{}'. Commit it so consumers are offered", opts.cache);
            println!("the cache automatically:  git add flake.nix && git commit");
        }
        None => {
            println!();
            println!("Pushed. To offer this cache to consumers automatically, re-run with the");
            println!("cache's public key (from its page at https://app.cachix.org):");
            println!(
                "  cargo xtask publish-toolchain-binaries {} --public-key {}.cachix.org-1:…",
                opts.cache, opts.cache
            );
        }
    }
    Ok(())
}

/// Uncomment and fill the `nixConfig` cache block in flake.nix.
fn fill_nixconfig(root: &Path, cache: &str, public_key: &str) -> Result<(), String> {
    let path = root.join("flake.nix");
    let contents = fs::read_to_string(&path).map_err(|e| format!("reading flake.nix: {e}"))?;
    let new = replace_nixconfig(&contents, cache, public_key)?;
    fs::write(&path, new).map_err(|e| format!("writing flake.nix: {e}"))
}

/// Pure part of `fill_nixconfig`: swap the commented template for a filled block.
fn replace_nixconfig(contents: &str, cache: &str, public_key: &str) -> Result<String, String> {
    if !contents.contains(NIXCONFIG_COMMENTED) {
        return Err(
            "the commented nixConfig template wasn't found in flake.nix (already filled, or \
             the file changed) — edit it by hand"
                .into(),
        );
    }
    let filled = format!(
        "  nixConfig = {{\n    extra-substituters = [ \"https://{cache}.cachix.org\" ];\n    extra-trusted-public-keys = [ \"{public_key}\" ];\n  }};"
    );
    Ok(contents.replace(NIXCONFIG_COMMENTED, &filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_publish_args() {
        let ok = parse_publish_args(&["mycache".to_string()]).unwrap();
        assert_eq!(ok.cache, "mycache");
        assert!(ok.public_key.is_none());

        let withkey = parse_publish_args(&[
            "mycache".to_string(),
            "--public-key".to_string(),
            "mycache.cachix.org-1:AAAA".to_string(),
        ])
        .unwrap();
        assert_eq!(withkey.public_key.as_deref(), Some("mycache.cachix.org-1:AAAA"));

        assert!(parse_publish_args(&[]).is_err()); // missing name
        assert!(parse_publish_args(&["a".to_string(), "b".to_string()]).is_err()); // two positionals
        assert!(parse_publish_args(&["c".to_string(), "--public-key".to_string()]).is_err()); // dangling value
    }

    #[test]
    fn fills_nixconfig_block() {
        let flake = format!("{{\n{NIXCONFIG_COMMENTED}\n  inputs = {{}};\n}}\n");
        let out = replace_nixconfig(&flake, "mycache", "mycache.cachix.org-1:KEY==").unwrap();
        assert!(out.contains("extra-substituters = [ \"https://mycache.cachix.org\" ]"));
        assert!(out.contains("extra-trusted-public-keys = [ \"mycache.cachix.org-1:KEY==\" ]"));
        assert!(!out.contains('#')); // the block is uncommented
        assert!(!out.contains("YOUR-CACHE")); // no placeholders left
        // idempotence guard: a file without the template is an error, not a silent no-op
        assert!(replace_nixconfig("no template here", "c", "k").is_err());
    }
}
