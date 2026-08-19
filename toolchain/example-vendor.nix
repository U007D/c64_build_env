# Fixed-output derivation: the bundled example's dependency tree, vendored for
# the offline `checks` build.
#
# Why this exists: bin/hello_world depends on c64_pac (git) and on crates.io
# crates, and the check sandbox has no network. A fixed-output derivation is the
# one place allowed to fetch — its pinned hash is the contract — so the sources
# land in the store here and the check builds against them.
#
# The inputs are the manifests and the lockfile, nothing else: vendoring depends
# on the dependency graph, not on the example's code, so editing main.rs must not
# invalidate this. cargo does refuse to load a member manifest with no target
# file, hence the stub written below.
#
# Re-pin (reset the hash to lib.fakeHash, then `cargo xprefetch-hashes`) whenever
# the example's dependencies change — until then a change fails loudly with a
# hash mismatch rather than drifting silently.
{
  lib,
  stdenvNoCC,
  git,
  cacert,
  rust-mos-stage0,
  pins,
}:
stdenvNoCC.mkDerivation {
  pname = "rust-mos-check-vendor";
  version = "hello-world";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../bin/hello_world/Cargo.toml
    ];
  };

  outputHash = pins.example-vendor-hash;
  outputHashAlgo = "sha256";
  outputHashMode = "recursive";

  nativeBuildInputs = [
    git
    rust-mos-stage0
  ];

  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  GIT_SSL_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  CARGO_HTTP_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall
    export HOME=$TMPDIR
    export CARGO_HOME=$TMPDIR/cargo-home

    # A workspace member with no src/lib.rs, src/main.rs, [lib] or [[bin]] fails
    # to parse, so give every member the stub cargo insists on. The vendor tree
    # is unaffected by what the file contains.
    for manifest in bin/*/Cargo.toml; do
      mkdir -p "''${manifest%/Cargo.toml}/src"
      : > "''${manifest%/Cargo.toml}/src/main.rs"
    done

    # --versioned-dirs matches the rust-mos-src vendor tree's layout, so the
    # check can merge the two directories (see check.nix).
    cargo vendor --versioned-dirs --locked vendor > vendor-config.toml

    mkdir -p $out
    cp -r vendor $out/vendor
    cp vendor-config.toml $out/vendor-config.toml
    runHook postInstall
  '';
}
