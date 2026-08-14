# rust-mos: mrk-its' rustc 1.87.0-dev fork with the MOS backend, built from
# source against the Nix llvm-mos, bootstrapped by the exact stage0 its
# src/stage0 pins. Follows nixpkgs' rustc.nix pattern (configure.py + x.py),
# with the nixpkgs fork-specific patches dropped (they target release
# tarballs of stock rustc and do not apply here).
#
# Installed layout (single output):
#   bin/rustc, bin/rustdoc
#   bin/cargo                      <- mrk-its' forked cargo (submodule
#                                     src/tools/cargo, branch
#                                     compiler_builtins_patched). REQUIRED:
#                                     it injects the mos compiler_builtins
#                                     into -Zbuild-std resolution. A stock
#                                     cargo must never shadow it.
#   lib/rustlib/src/rust/library   <- rust-src for -Zbuild-std (with the
#                                     c_uint fix and the rev-pinned
#                                     compiler_builtins patch)
#   targets/mos-{sim,c64,a800xl}-none.json  <- RUST_TARGET_PATH points here
{
  lib,
  stdenv,
  python3,
  cmake,
  pkg-config,
  openssl,
  zlib,
  curl,
  libiconv,
  llvm-mos,
  rust-mos-stage0,
  rust-mos-src,
  pins,
}:
let
  triple = stdenv.hostPlatform.rust.rustcTarget;
in
stdenv.mkDerivation {
  pname = "rust-mos";
  version = pins.rust-mos.version;

  src = rust-mos-src;

  env = {
    LIBZ_SYS_STATIC = "1";
  };

  nativeBuildInputs = [
    python3
    cmake # libgit2-sys (forked cargo build)
    pkg-config
  ];
  buildInputs = [
    openssl
    zlib
    curl
  ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [ libiconv ];

  # Everything is pre-vendored in rust-mos-src; hard-fail on any network use.
  CARGO_NET_OFFLINE = "true";

  # rustc links llvm-mos' shared libLLVM.dylib (@rpath/libLLVM.dylib, from
  # --enable-llvm-link-shared). The freshly built stageN rustc's only rpath is
  # @loader_path/../lib, so during the build bootstrap can't run it. Point dyld
  # at llvm-mos' lib (harmless/ignored off Darwin). The installed rustc is
  # handled by the libLLVM symlink in postInstall.
  DYLD_FALLBACK_LIBRARY_PATH = "${llvm-mos}/lib";

  postPatch = ''
    patchShebangs src/etc x.py 2>/dev/null || true

    # --- the c_uint gate (critical correctness item) ----------------------
    # llvm-mos C int is 16-bit and the target spec says c_int_width = "16",
    # but this generation ships core::ffi with c_int/c_uint = 32-bit (the
    # 1.78-era patch was not carried over the rebase; verified at 8f3a80f8).
    # Add mos to the 16-bit cfg list, with count guards.
    prim=library/core/src/ffi/primitives.rs
    before=$(grep -c 'any(target_arch = "avr", target_arch = "msp430")' "$prim" || true)
    if [ "$before" -lt 1 ]; then
      echo "GUARD FAILED: 16-bit c_int cfg not found in $prim - fork layout changed, refusing to build" >&2
      exit 1
    fi
    sed -i 's/any(target_arch = "avr", target_arch = "msp430")/any(target_arch = "avr", target_arch = "msp430", target_arch = "mos")/g' "$prim"
    after=$(grep -c 'any(target_arch = "avr", target_arch = "msp430", target_arch = "mos")' "$prim" || true)
    if [ "$after" -ne "$before" ]; then
      echo "GUARD FAILED: c_uint patch applied $after/$before times in $prim" >&2
      exit 1
    fi
    echo "c_uint gate: patched $after cfg site(s) in $prim"

    # --- config.example.toml TOML fix -------------------------------------
    # configure.py derives config.toml from config.example.toml. Under
    # [target.x86_64-unknown-linux-gnu] the fork left two lines UNcommented:
    # a hardcoded `llvm-config = "../llvm-mos/build/bin/llvm-config"` and
    # `llvm-has-rust-patches = if llvm-config { false } else { true }` — the
    # latter is a pseudo-expression, not valid TOML, so the generated config
    # fails to parse. Re-comment both; our --set flags supply the real
    # llvm-config for the host triple we actually build.
    sed -i \
      -e 's|^llvm-config = "\.\./llvm-mos/build/bin/llvm-config"|#&|' \
      -e 's|^llvm-has-rust-patches = if llvm-config|#&|' \
      config.example.toml
    if grep -qE '^[^#].*= if ' config.example.toml; then
      echo "GUARD FAILED: config.example.toml still has an uncommented non-TOML 'if' line" >&2
      grep -nE '^[^#].*= if ' config.example.toml >&2
      exit 1
    fi

    # --- check-cfg allowlist for the mos target_arch -----------------------
    # The c_uint patch above introduces cfg(target_arch = "mos") in
    # library/core. The stage0 compiler (stock beta) doesn't know the mos
    # target, so its check-cfg flags "mos" as an unexpected cfg value and
    # -D warnings makes it fatal. Declare it in core's own check-cfg allowlist
    # (exactly the fix rustc suggests). Harmless at stage1/2 where mos is known.
    substituteInPlace library/core/Cargo.toml --replace-fail \
      "check-cfg = [" \
      "check-cfg = [
    'cfg(target_arch, values(\"mos\"))',"

    # --- mark the source as a distributed tarball -------------------------
    # x.py steps require_submodule() several submodules we deliberately don't
    # vendor (src/llvm-project, src/doc/*, src/tools/rustc-perf, …). With a
    # stripped .git, bootstrap treats the tree as "neither git nor tarball" and
    # those checks are fatal (they only pass for real tarballs or git clones).
    # Record a git-commit-info file so bootstrap classifies this as a released
    # source tarball (GitInfo::RecordedForTarball) — which is exactly what a
    # vendored source is — and require_submodule() then no-ops for every
    # submodule. Lines: full sha, short sha, commit date.
    printf '%s\n%s\n%s\n' \
      '${pins.rust-mos.rev}' \
      '${builtins.substring 0 9 pins.rust-mos.rev}' \
      '2025-03-08' > git-commit-info

    # --- skip GenerateCopyright in dist::Rustc ----------------------------
    # `x.py install` builds the rustc dist tarball, which bundles copyright HTML
    # generated by GenerateCopyright. That step runs `cargo metadata` over EVERY
    # tool workspace (rust-analyzer, clippy, …) to collect licenses — but we
    # only build rustc/rustdoc/cargo and only vendor those workspaces, so it
    # fails offline. The copyright HTML is cosmetic; return an empty file list so
    # the step is skipped without pulling in unvendored tool workspaces.
    substituteInPlace src/bootstrap/src/core/build_steps/dist.rs --replace-fail \
      "let file_list = builder.ensure(super::run::GenerateCopyright);" \
      "let file_list: Vec<std::path::PathBuf> = Vec::new();"
  ''
  + lib.optionalString stdenv.hostPlatform.isDarwin ''
    # nixpkgs carries this substitution for Darwin: rustc shells out to
    # /usr/bin/strip when stripping dylibs; point it at llvm-strip instead.
    # Guarded: skip silently if the fork's code drifted.
    if grep -q '"/usr/bin/strip"' compiler/rustc_codegen_ssa/src/back/link.rs; then
      substituteInPlace compiler/rustc_codegen_ssa/src/back/link.rs \
        --replace-fail '"/usr/bin/strip"' '"${llvm-mos}/bin/llvm-strip"'
    fi
  '';

  # rustc's configure rejects autotools-style --build/--host values; pass
  # rust triples explicitly (nixpkgs does the same).
  configurePlatforms = [ ];
  configurePhase = ''
    runHook preConfigure
    # The fork commits its own config.toml (the maintainers' Linux CI config:
    # /tmp build-dir, linux host triples, hardcoded /usr/local llvm-config and
    # cross toolchains). configure.py refuses to run when a config.toml exists,
    # and none of those settings apply to this Nix build — we regenerate the
    # whole config from the flags below. Nothing MOS-specific lives there.
    rm -f config.toml
    python3 src/bootstrap/configure.py \
      --prefix=$out \
      --sysconfdir=$out/etc \
      --build=${triple} --host=${triple} --target=${triple} \
      --set=build.rustc=${rust-mos-stage0}/bin/rustc \
      --set=build.cargo=${rust-mos-stage0}/bin/cargo \
      --set=build.rustfmt=${rust-mos-stage0}/bin/rustfmt \
      --enable-vendor \
      --enable-extended \
      --tools=rustc,rustdoc,cargo \
      --disable-docs \
      --enable-rpath \
      --enable-llvm-link-shared \
      --set=llvm.download-ci-llvm=false \
      --set=build.optimized-compiler-builtins=false \
      --set=target.${triple}.llvm-config=${llvm-mos}/bin/llvm-config
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild
    python3 x.py build --stage 2 -j $NIX_BUILD_CORES
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    python3 x.py install --stage 2 -j $NIX_BUILD_CORES
    runHook postInstall
  '';

  postInstall = ''
    # --- libLLVM for the installed rustc (Darwin) -------------------------
    # rustc and librustc_driver load libLLVM via @rpath/libLLVM.dylib with only
    # @loader_path/../lib on their rpath, i.e. $out/lib. Link llvm-mos' shared
    # libLLVM there so the installed toolchain runs. The `-e` guard makes this a
    # no-op off Darwin (there it's libLLVM.so, found via the --enable-rpath path).
    #
    # The LLVM tools under rustlib/${triple}/bin (rust-objcopy, rust-lld, …) share
    # that @rpath/libLLVM.dylib dependency but with a DIFFERENT rpath:
    # @loader_path/../lib resolves to rustlib/${triple}/lib, not $out/lib. With no
    # libLLVM there, `strip = true` invokes rust-objcopy and it SIGABRTs (dyld
    # can't load libLLVM), failing debug-info stripping. Symlink it in that dir too.
    if [ -e ${llvm-mos}/lib/libLLVM.dylib ]; then
      ln -s ${llvm-mos}/lib/libLLVM.dylib $out/lib/libLLVM.dylib
      mkdir -p $out/lib/rustlib/${triple}/lib
      ln -s ${llvm-mos}/lib/libLLVM.dylib $out/lib/rustlib/${triple}/lib/libLLVM.dylib
    fi

    # --- rust-src for -Zbuild-std -----------------------------------------
    # Same content the docker image got from the rust-src dist tarball:
    # the (patched) library workspace, locks included. compiler_builtins in
    # its [patch.crates-io] is already rev-pinned by rust-mos-src.
    srcdir=$out/lib/rustlib/src/rust
    mkdir -p "$srcdir/src/llvm-project/libunwind"
    rm -rf "$srcdir/library" # in case x.py install already placed one
    cp -r library "$srcdir/library"

    # --- mos-*-none target JSONs (RUST_TARGET_PATH) ------------------------
    # Replica of upstream create_mos_targets.py; see mos-targets.py.
    $out/bin/rustc --target mos-unknown-none \
      -Z unstable-options --print target-spec-json > mos-spec.json
    python3 ${./mos-targets.py} mos-spec.json $out/targets

    # x.py install bookkeeping not needed downstream
    rm -rf $out/lib/rustlib/{install.log,uninstall.sh,rust-installer-version,manifest-*,components}
  '';

  # rustc rlibs contain bitcode-ish sections strip would mangle; nixpkgs
  # also avoids stripping rustc's lib dir.
  stripExclude = [ "*.rlib" ];

  enableParallelBuilding = true;
  requiredSystemFeatures = [ "big-parallel" ];

  passthru = {
    inherit llvm-mos rust-mos-stage0 rust-mos-src;
    targetsDir = "targets";
  };

  meta = {
    description = "rustc 1.87.0-dev with the LLVM-MOS 6502 backend (mrk-its fork) + forked cargo";
    homepage = "https://github.com/mrk-its/rust-mos";
    license = with lib.licenses; [
      mit
      asl20
    ];
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
      "x86_64-darwin"
    ];
  };
}
