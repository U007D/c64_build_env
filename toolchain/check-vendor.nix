# Fixed-output derivation: vendored compiler_builtins from mrk-its' fork,
# branch mos-0.1.150.
#
# Why this exists: the forked cargo's standard_lib.rs injects
#   https://github.com/mrk-its/compiler-builtins?branch=mos-0.1.150#0.1.150
# into every -Zbuild-std resolution (verified at mrk-its/cargo
# compiler_builtins_patched @ a2df727d). For the offline `checks` build that
# git source must be vendored under exactly that source id — branch form, not
# rev form — so the dummy manifest below uses branch = "mos-0.1.150".
#
# Determinism note (labeled assumption): the branch tip is 090b8f3c... and
# mrk-its has not pushed to it since 2025-03. If the branch ever moves, this
# FOD fails loudly with a hash mismatch rather than drifting silently.
{
  stdenvNoCC,
  git,
  cacert,
  rust-mos-stage0,
  pins,
}:
stdenvNoCC.mkDerivation {
  pname = "rust-mos-check-vendor";
  version = "compiler-builtins-mos-0.1.150";

  outputHash = pins.check-vendor-hash;
  outputHashAlgo = "sha256";
  outputHashMode = "recursive";

  nativeBuildInputs = [
    git
    rust-mos-stage0
  ];

  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  GIT_SSL_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  CARGO_HTTP_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall
    export HOME=$TMPDIR
    export CARGO_HOME=$TMPDIR/cargo-home

    mkdir dummy && cd dummy
    mkdir src
    echo '#![no_std]' > src/lib.rs
    cat > Cargo.toml <<'EOF'
    [package]
    name = "vendor-dummy"
    version = "0.0.0"
    edition = "2021"

    [dependencies]
    compiler_builtins = { git = "https://github.com/mrk-its/compiler-builtins", branch = "mos-0.1.150" }
    EOF

    cargo vendor --versioned-dirs vendor > vendor-config.toml

    mkdir -p $out
    cp -r vendor $out/vendor
    cp vendor-config.toml $out/vendor-config.toml
    runHook postInstall
  '';
}
