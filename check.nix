# checks.<system>.c64-hello-world: fully offline, verify that
#   * the toolchain's mos ABI is intact — `c_uint` is 16-bit, asserted at compile
#     time by a probe this check writes below. The probe lives here, not in any
#     project's source: the mos ABI is a property of the build environment, and
#     no project should have to carry the environment's own test;
#   * the two llvm-mos bugs the repo works around are still present — see
#     expect_broken below. When a bug is fixed upstream the check FAILS, naming
#     the workaround to delete, so a stale workaround cannot outlive its cause.
#     Nix re-runs this check when its inputs change, so the signal arrives on the
#     toolchain bump that fixes the bug;
#   * the bundled example (bin/hello_world) builds to a C64 PRG whose first two
#     bytes are the $0801 load address (01 08) the SDK's C64 link step emits
#     ahead of the BASIC SYS stub.
#
# Offline story — three vendor trees, since the sandbox has no network:
#   * compiler_builtins(mos-0.1.148, rev-pinned) and the crates.io deps of the
#     rust-mos workspaces come from rust-mos-src's captured vendor tree/config;
#   * compiler_builtins(mos-0.1.150) — injected by the forked cargo into every
#     -Zbuild-std resolution — comes from check-vendor;
#   * c64_pac and the example's own crates.io deps come from example-vendor.
{
  lib,
  stdenv,
  rust-mos,
  rust-mos-src,
  mos-toolchain,
  check-vendor,
  example-vendor,
}:
stdenv.mkDerivation {
  pname = "rust-mos-check-c64-hello-world";
  version = "0.1.0";

  # The whole repo: hello_world is a workspace member, so its build needs the
  # root Cargo.toml (profiles) and Cargo.lock alongside it. Flakes see only
  # git-tracked files, so target/ and untracked bin/ projects stay out.
  src = ./.;

  nativeBuildInputs = [
    rust-mos
    mos-toolchain
  ];

  # Belt and suspenders: the sandbox has no network anyway.
  CARGO_NET_OFFLINE = "true";
  RUST_TARGET_PATH = "${rust-mos}/targets";

  configurePhase = ''
    runHook preConfigure
    export CARGO_HOME=$TMPDIR/cargo-home
    mkdir -p $CARGO_HOME .cargo

    # Cargo allows exactly one directory source to stand in for crates.io, so the
    # rust-mos and example trees have to become one directory rather than two
    # entries. One symlink per crate directory: both trees were vendored with
    # --versioned-dirs, so a name collides only when it is the same crate at the
    # same version — identical content, and the first one linked wins.
    mkdir -p vendor-merged
    ln -s ${rust-mos-src}/vendor/* vendor-merged/
    for crate in ${example-vendor}/vendor/*; do
      [ -e "vendor-merged/$(basename "$crate")" ] || ln -s "$crate" vendor-merged/
    done

    # Start from the source-replacement config cargo vendor emitted for the
    # rust-mos workspaces (it names every git source id exactly), retarget its
    # relative vendor dir at the merged tree...
    sed -e 's|^directory = "vendor"$|directory = "'"$PWD"'/vendor-merged"|' \
      ${rust-mos-src}/.cargo/config.toml > .cargo/config.toml

    # ...add the example's git source (c64_pac), which resolves in the same
    # merged tree, dropping the [source.crates-io] and [source.vendored-sources]
    # blocks the config above already defines (TOML forbids redefining a table)...
    awk 'BEGIN{skip=0}
         /^\[source\.crates-io\]$/{skip=1; next}
         /^\[source\.vendored-sources\]$/{skip=1; next}
         /^\[/{skip=0}
         skip==0{print}' ${example-vendor}/vendor-config.toml >> .cargo/config.toml

    # ...then add the mos-0.1.150 source the forked cargo injects, under a
    # distinct replacement name, from its own directory.
    awk 'BEGIN{skip=0}
         /^\[source\.crates-io\]$/{skip=1; next}
         /^\[/{skip=0}
         skip==0{print}' ${check-vendor}/vendor-config.toml \
      | sed -e 's|vendored-sources|builtins-150-vendor|g' \
            -e 's|^directory = "vendor"$|directory = "${check-vendor}/vendor"|' \
      >> .cargo/config.toml

    {
      echo '[net]'
      echo 'offline = true'
    } >> .cargo/config.toml

    echo "==== .cargo/config.toml ===="
    cat .cargo/config.toml
    echo "============================"
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild

    # mos-mega65-none is this repo's own spec (targets/), unlike mos-c64-none,
    # which ships with rust-mos. rustc finds both through RUST_TARGET_PATH.
    export RUST_TARGET_PATH="$RUST_TARGET_PATH:$PWD/targets"

    # Every probe below is the same bare-metal crate; only the profile, the
    # target and the expected outcome differ. `[workspace]` keeps a probe out of
    # the repo workspace it is nested in, so the profile written here is the one
    # cargo applies.
    write_probe() {
      probe_dir=$1
      probe_profile=$2
      probe_extra=$3
      mkdir -p "$probe_dir/src"
      {
        echo '#![no_std]'
        echo '#![no_main]'
        echo 'use core::panic::PanicInfo;'
        printf '%s\n' "$probe_extra"
        echo '#[panic_handler]'
        echo 'fn panic(_: &PanicInfo) -> ! { loop {} }'
        echo '#[unsafe(no_mangle)]'
        echo 'extern "C" fn main() {}'
      } > "$probe_dir/src/main.rs"
      {
        printf '[package]\nname = "probe"\nversion = "0.0.0"\nedition = "2024"\n\n[workspace]\n\n'
        printf '%s\n' "$probe_profile"
      } > "$probe_dir/Cargo.toml"
    }

    # A regression probe: compile the configuration a workaround in this repo
    # exists to avoid. Failing with the known signature is the expected outcome,
    # and keeps that workaround justified. Compiling successfully means the
    # upstream bug is fixed — so the check fails, loudly, naming the workaround
    # to delete. Failing for any other reason fails too: a probe that stops
    # testing what it claims to test is worse than no probe.
    expect_broken() {
      probe_label=$1
      probe_dir=$2
      probe_signature=$3
      probe_workaround=$4
      shift 4
      if (cd "$probe_dir" && "$@") > "$probe_dir/probe.log" 2>&1; then
        echo "FIXED UPSTREAM: $probe_label" >&2
        echo "  This compiles now, so the workaround is obsolete: $probe_workaround" >&2
        echo "  Remove the workaround, then remove this probe from check.nix." >&2
        exit 1
      fi
      if ! grep -q "$probe_signature" "$probe_dir/probe.log"; then
        echo "UNEXPECTED: probe '$probe_label' failed without '$probe_signature'." >&2
        echo "  It no longer tests the bug the workaround addresses. Log tail:" >&2
        tail -30 "$probe_dir/probe.log" >&2
        exit 1
      fi
      echo "OK: still broken upstream, workaround stays — $probe_label"
    }

    # The profiles the repo's own Cargo.toml sets, which the probes below either
    # reproduce or strip a workaround out of.
    release_profile=$(printf '[profile.release]\nopt-level = 3\nlto = "fat"\npanic = "abort"\nstrip = true\n')
    dev_profile=$(printf '[profile.dev]\nopt-level = 2\nlto = "thin"\n')

    # 1. The environment's ABI gate, which must compile. `c_uint` is 16-bit under
    #    the mos ABI and 32-bit on every host this builds on, so the assert fails
    #    if the target spec, the SDK or the fork stops delivering the mos ABI —
    #    a compile error here, rather than a program that misbehaves on hardware.
    write_probe abi-probe "$release_profile" \
      'use core::{ffi::c_uint, mem::size_of};
       const _: () = assert!(size_of::<c_uint>() == 2, "mos c_uint must be 16-bit");'
    for triple in mos-c64-none mos-mega65-none; do
      (cd abi-probe && cargo build --release --target "$triple" \
         -Zbuild-std=core,alloc -Zbuild-std-features=panic_immediate_abort)
      echo "OK: mos ABI probe compiled for $triple (c_uint is 16-bit)"
    done

    # 2. The release profile without `lto = "fat"`. Eager dependency codegen hands
    #    llvm-mos the 128-bit remainder in `core::fmt::num::exp_u128` — a function
    #    no 6502 program calls — and the backend aborts legalizing it.
    write_probe lto-probe "$(printf '[profile.release]\nopt-level = 3\npanic = "abort"\nstrip = true\n')" ""
    expect_broken \
      'llvm-mos cannot legalize core::fmt::num::exp_u128 (needs fat LTO to stay dead)' \
      lto-probe \
      'unable to legalize instruction' \
      'lto = "fat" in [profile.release], Cargo.toml' \
      cargo build --release --target mos-c64-none \
        -Zbuild-std=core,alloc -Zbuild-std-features=panic_immediate_abort

    # 3. The dev profile without the dependency-scoped overflow-checks override.
    #    The checked-arithmetic branches cargo inserts by default in dev defeat
    #    the 45GS02 register allocator, and rustc dies with a SIGSEGV inside
    #    LLVM's RAGreedy compiling core / compiler_builtins. MEGA65 only, and
    #    unrelated to the ABI: the c_uint assert above compiles for this target
    #    too. `cargo check` is enough, since the crash is in a dependency.
    write_probe overflow-probe "$dev_profile" ""
    expect_broken \
      'llvm-mos 45GS02 register allocator crashes on dev overflow checks' \
      overflow-probe \
      'SIGSEGV' \
      'Target::workarounds for Mega65, xtask/src/target.rs' \
      cargo check --target mos-mega65-none \
        -Zbuild-std=core,alloc -Zbuild-std-features=panic_immediate_abort

    # The bundled example, from its vendored dependencies, with the same flags
    # `cargo xbuild` passes (xtask/src/target.rs) — so the check exercises the
    # environment's own build rather than a variation on it.
    cargo build --release --target mos-c64-none \
      -Zbuild-std=core,alloc -Zbuild-std-features=panic_immediate_abort \
      -p hello_world
    runHook postBuild
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck
    # The ABI gate is a const assert, so reaching a linked probe is the pass.
    for triple in mos-c64-none mos-mega65-none; do
      probe=abi-probe/target/$triple/release/probe
      test -f "$probe" || { echo "no linked output at $probe"; exit 1; }
    done

    prg=target/mos-c64-none/release/hello_world
    test -f "$prg" || { echo "no linked output at $prg"; ls -R target; exit 1; }
    head=$(head -c 2 "$prg" | od -An -tx1 | tr -d ' ')
    if [ "$head" != "0108" ]; then
      echo "FAIL: output does not start with PRG load address \$0801 (got: $head)" >&2
      exit 1
    fi
    size=$(stat -c %s "$prg" 2>/dev/null || stat -f %z "$prg")
    echo "OK: PRG with load address \$0801, $size bytes"
    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp target/mos-c64-none/release/hello_world $out/hello_world.prg
    # Keep the ELF-with-debug twin if the SDK link produced one.
    for f in target/mos-c64-none/release/hello_world.elf; do
      [ -f "$f" ] && cp "$f" $out/ || true
    done
    runHook postInstall
  '';

  dontFixup = true;

  meta.description = "Offline C64 PRG smoke test (\$0801 header) for rust-mos";
}
