# checks.<system>.c64-hello-world: build the bundled example (c64/examples/hello-world)
# to a C64 PRG, fully offline, and verify:
#   * the c_uint == 2 bytes const assert compiled (compile failure otherwise);
#   * the output starts with the $0801 load address (01 08) the SDK's C64
#     link step emits ahead of the BASIC SYS stub.
#
# Offline story:
#   * crates.io + compiler_builtins(mos-0.1.148, rev-pinned) come from
#     rust-mos-src's captured vendor tree/config;
#   * compiler_builtins(mos-0.1.150) — injected by the forked cargo into
#     every -Zbuild-std resolution — comes from check-vendor.
{
  lib,
  stdenv,
  rust-mos,
  rust-mos-src,
  mos-toolchain,
  check-vendor,
}:
stdenv.mkDerivation {
  pname = "rust-mos-check-c64-hello-world";
  version = "0.1.0";

  src = ./examples/hello-world;

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

    # Start from the source-replacement config cargo vendor emitted for the
    # rust-mos workspaces (it names every git source id exactly), retarget its
    # relative vendor dir at the store path...
    sed -e 's|^directory = "vendor"$|directory = "${rust-mos-src}/vendor"|' \
      ${rust-mos-src}/.cargo/config.toml > .cargo/config.toml

    # ...then add the mos-0.1.150 source the forked cargo injects, under a
    # distinct replacement name. Drop the duplicate [source.crates-io] block
    # from the second config first (TOML forbids redefining a table).
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
    cargo build --release --target mos-c64-none -Zbuild-std=core,alloc
    runHook postBuild
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck
    prg=target/mos-c64-none/release/hello-world
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
    cp target/mos-c64-none/release/hello-world $out/hello-world.prg
    # Keep the ELF-with-debug twin if the SDK link produced one.
    for f in target/mos-c64-none/release/hello-world.elf; do
      [ -f "$f" ] && cp "$f" $out/ || true
    done
    runHook postInstall
  '';

  dontFixup = true;

  meta.description = "Offline C64 PRG smoke test (c_uint gate + \$0801 header) for rust-mos";
}
