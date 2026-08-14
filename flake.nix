{
  description = "rust-mos C64 toolchain, built from source: llvm-mos (6502 backend) + llvm-mos-sdk + rustc 1.87.0-dev fork, native per platform";

  # After creating your Cachix cache, uncomment and fill these in so
  # downstream users are offered your cache automatically (see README):
  nixConfig = {
    extra-substituters = [ "https://u007d-rust-mos.cachix.org" ];
    extra-trusted-public-keys = [ "u007d-rust-mos.cachix.org-1:dfXYUvgjBRiBwyP/LmYP5YvKKp8uFiAT19kMaCRtWJU=" ];
  };

  inputs = {
    # Pinned to an exact commit (tip of nixos-25.05 at 2026-07-28), not the moving
    # branch, so the build is identical even if flake.lock is ever lost or regenerated.
    # To update deliberately: bump this rev, then `nix flake lock`.
    nixpkgs.url = "github:NixOS/nixpkgs/ac62194c3917d5f474c1a844b6fd6da2db95077d";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "aarch64-darwin" # primary
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin" # untested, hashes present
      ];
      eachSystem = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      pins = import ./toolchain/pins.nix;
    in
    {
      packages = eachSystem (
        pkgs:
        let
          call = pkgs.lib.callPackageWith (pkgs // toolchain // { inherit pins; });
          toolchain = rec {
            rust-mos-stage0 = call ./toolchain/stage0.nix { };
            llvm-mos = call ./toolchain/llvm-mos.nix { };
            llvm-mos-sdk = call ./toolchain/llvm-mos-sdk.nix { };
            mos-toolchain = call ./toolchain/mos-toolchain.nix { };
            rust-mos-src = call ./toolchain/rust-mos-src.nix { };
            rust-mos = call ./toolchain/rust-mos.nix { };
            check-vendor = call ./toolchain/check-vendor.nix { };
          };
        in
        {
          inherit (toolchain)
            llvm-mos
            llvm-mos-sdk
            mos-toolchain
            rust-mos
            ;
          default = toolchain.rust-mos;

          # Internal/plumbing attrs, exposed for prefetch-hashes.sh and debugging:
          stage0 = toolchain.rust-mos-stage0;
          rust-mos-src = toolchain.rust-mos-src;
          check-vendor = toolchain.check-vendor;
          llvm-mos-source = toolchain.llvm-mos.src;
          llvm-mos-sdk-source = toolchain.llvm-mos-sdk.src;
        }
      );

      checks = eachSystem (
        pkgs:
        let
          p = self.packages.${pkgs.stdenv.hostPlatform.system};
        in
        {
          c64-hello-world = pkgs.callPackage ./c64/check.nix {
            inherit (p) rust-mos rust-mos-src mos-toolchain check-vendor;
          };
        }
      );

      devShells = eachSystem (
        pkgs:
        let
          p = self.packages.${pkgs.stdenv.hostPlatform.system};

          # The repo's xtask binary, built with stock nixpkgs Rust (it is
          # dependency-free, std-only — nothing to vendor). Used to back the
          # `cargo-x*` shims below; the host `cargo xtask` alias is unaffected.
          xtask = pkgs.rustPlatform.buildRustPackage {
            pname = "xtask";
            version = "0.1.0";
            src = ./xtask;
            cargoLock.lockFile = ./xtask/Cargo.lock;
          };

          # `cargo <name>` runs any `cargo-<name>` on PATH, so this makes
          # `cargo xasm [FUNCTION]` work from any crate directory in the shell —
          # the run-from-the-crate ergonomics a cargo alias can't give (an alias
          # resolves `--manifest-path` against the cwd, so it only works from the
          # repo root). cargo may pass the subcommand name through as the first
          # arg; drop a leading "xasm", then defer to xtask's asm subcommand.
          cargo-xasm = pkgs.writeShellScriptBin "cargo-xasm" ''
            [ "''${1:-}" = "xasm" ] && shift
            exec ${xtask}/bin/xtask asm "$@"
          '';

          # `cargo xrun` — build the current C64 crate in release and launch it in
          # VICE (release: a debug build usually won't fit in RAM). Same PATH-shim
          # trick as cargo-xasm, for the same reason (the `cargo xtask` alias is
          # repo-root-only). Runs `xtask run`, which shells out to `cargo run --release`.
          cargo-xrun = pkgs.writeShellScriptBin "cargo-xrun" ''
            [ "''${1:-}" = "xrun" ] && shift
            exec ${xtask}/bin/xtask run "$@"
          '';

          # `cargo xbuild` / `cargo xcheck` — cross-compile / type-check the current
          # C64 crate. Same PATH-shim trick as cargo-xasm. Each runs `xtask build`/
          # `xtask check`, which pass `--target mos-c64-none -Zbuild-std=…` explicitly
          # (the repo's `.cargo/config.toml` no longer sets a default mos target, so
          # `cargo xtask` itself builds for the host — see .cargo/config.toml).
          cargo-xbuild = pkgs.writeShellScriptBin "cargo-xbuild" ''
            [ "''${1:-}" = "xbuild" ] && shift
            exec ${xtask}/bin/xtask build "$@"
          '';
          cargo-xcheck = pkgs.writeShellScriptBin "cargo-xcheck" ''
            [ "''${1:-}" = "xcheck" ] && shift
            exec ${xtask}/bin/xtask check "$@"
          '';

          # Repo automation, same PATH-shim trick so they run from any directory in
          # the shell (not just the repo root the `cargo xtask` alias is limited to).
          # Unlike the build commands above, these pass no mos flags — they drive `nix`.
          cargo-xpublish-toolchain-binaries = pkgs.writeShellScriptBin "cargo-xpublish-toolchain-binaries" ''
            [ "''${1:-}" = "xpublish-toolchain-binaries" ] && shift
            exec ${xtask}/bin/xtask publish-toolchain-binaries "$@"
          '';
          cargo-xprefetch-hashes = pkgs.writeShellScriptBin "cargo-xprefetch-hashes" ''
            [ "''${1:-}" = "xprefetch-hashes" ] && shift
            exec ${xtask}/bin/xtask prefetch-hashes "$@"
          '';
        in
        {
          default = pkgs.mkShell {
            # rust-mos FIRST: its rustc/cargo (the forked cargo) must win.
            # The cargo-x* shims are distinct binaries, so they never shadow cargo.
            packages = [
              p.rust-mos
              p.mos-toolchain
              cargo-xasm
              cargo-xrun
              cargo-xbuild
              cargo-xcheck
              cargo-xpublish-toolchain-binaries
              cargo-xprefetch-hashes
            ]
            # VICE (x64sc) backs `cargo run` for C64 crates (their
            # .cargo/config.toml sets `runner = ["x64sc", "-autostart"]`).
            # nixpkgs' `vice` is marked Linux-only (meta.platforms has no
            # darwin), so there it is a `brew install vice` prerequisite (see
            # README); on Linux the dev shell provides it.
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.vice ];

            RUST_TARGET_PATH = "${p.rust-mos}/targets";
            RUST_SRC_PATH = "${p.rust-mos}/lib/rustlib/src/rust/library";

            shellHook = ''
              # Known footgun: a stock rustc/cargo (rustup shims, homebrew,
              # another nix shell) shadowing the mos toolchain. Force our
              # paths to the front and verify.
              export PATH="${p.rust-mos}/bin:${p.mos-toolchain}/bin:$PATH"
              # `nix develop` (unlike classic `nix-shell`) does not set IN_NIX_SHELL,
              # so interactive shells that key off it never react. In particular a
              # fish that hoists $fish_user_paths (~/.cargo/bin, …) to the front on
              # startup re-buries our store paths AFTER this hook ran; the usual
              # remedy is a config.fish that, when IN_NIX_SHELL is set, moves
              # /nix/store paths back to the front. Set it so that remedy fires.
              export IN_NIX_SHELL="''${IN_NIX_SHELL:-impure}"
              # rustup's shims resolve via these; make sure they can't hijack
              # `cargo build` inside this shell.
              unset RUSTUP_TOOLCHAIN RUSTUP_HOME CARGO 2>/dev/null || true
              # cargo merges .cargo/config.toml from EVERY ancestor directory, so a
              # personal `[build] rustc-wrapper = ".../clippy-driver"` sitting anywhere
              # above the crate (e.g. ~/Code/.cargo/config.toml) silently wraps rustc.
              # That wrapper is a rustup shim: with RUSTUP_TOOLCHAIN unset it dispatches
              # to the default *stable* rustc, which can't load the custom mos-c64-none
              # target ("custom targets are unstable"). An empty env var overrides any
              # such config and disables the wrapper for this shell.
              export CARGO_BUILD_RUSTC_WRAPPER="" CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER=""

              # macOS: VICE (x64sc) backing `cargo run` is a brew-installed GTK app.
              # It locates its compiled GSettings schemas via XDG_DATA_DIRS, but inside
              # `nix develop` that is narrowed to store paths, so GTK aborts at startup
              # with "No GSettings schemas are installed" (SIGTRAP) and `cargo run` dies.
              # Add brew's share dir (Apple Silicon /opt/homebrew, Intel /usr/local) so
              # the emulator can launch. No-op where brew is absent (e.g. Linux, where
              # VICE comes from nixpkgs and its schemas are already wired up).
              if command -v brew >/dev/null 2>&1; then
                __brew_share="$(brew --prefix)/share"
                if [ -e "$__brew_share/glib-2.0/schemas/gschemas.compiled" ]; then
                  export XDG_DATA_DIRS="$__brew_share:''${XDG_DATA_DIRS:-}"
                fi
                unset __brew_share
              fi

              if ! rustc --print target-list 2>/dev/null | grep -qx 'mos-unknown-none'; then
                echo "WARNING: the rustc on PATH is NOT rust-mos (no mos-unknown-none target)." >&2
                echo "         Something is shadowing ${p.rust-mos}/bin." >&2
              fi
              case "$(command -v cargo)" in
                ${p.rust-mos}/bin/cargo) ;;
                *)
                  echo "WARNING: cargo resolves to $(command -v cargo), not the forked rust-mos cargo." >&2
                  echo "         -Zbuild-std would silently use the wrong compiler_builtins." >&2
                  ;;
              esac

              echo "rust-mos $(rustc --version 2>/dev/null) | targets: $RUST_TARGET_PATH"
              printf '  %-33s  %s\n' \
                "cargo xbuild"                     "build the .prg   (plain 'cargo build' targets the host)" \
                "cargo xrun"                       "build, then launch in VICE" \
                "cargo xcheck"                     "type-check" \
                "cargo xasm"                       "show the mos assembly" \
                "cargo xpublish-toolchain-binaries" "push the toolchain to Cachix"
            '';
          };
        }
      );
    };
}
