# Fixed-output derivation: rust-mos source checkout, prepared for a fully
# offline build.
#
# Git checkouts of rustc have no vendor/ directory (unlike release tarballs),
# so this FOD — the only network-enabled step — does everything that needs
# the network, deterministically:
#   1. checkout rust-mos @ pinned rev + the three submodules the build needs
#      (src/tools/cargo = mrk-its' forked cargo; library/stdarch and
#      library/backtrace, path-dependencies of core/std);
#   2. pin the one *moving* reference in the tree: library/Cargo.toml patches
#      compiler_builtins to a git *branch* (mos-0.1.148); rewrite it to the
#      verified commit of that branch so re-vendoring can never drift;
#   3. sync library/Cargo.lock with that patch (the committed lock pins the
#      crates.io 0.1.148, out of sync with the git patch — verified);
#   4. `cargo vendor --sync` over every workspace the build touches, capturing
#      the emitted source-replacement config verbatim (it contains the git
#      source stanza that a hardcoded two-stanza config would miss).
#
# The main rust-mos derivation consumes this tree and builds with the network
# cut off.
{
  stdenvNoCC,
  git,
  cacert,
  rust-mos-stage0,
  pins,
}:
stdenvNoCC.mkDerivation {
  pname = "rust-mos-src-vendored";
  version = pins.rust-mos.version;

  outputHash = pins.rust-mos-src-hash;
  outputHashAlgo = "sha256";
  outputHashMode = "recursive";

  nativeBuildInputs = [
    git
    rust-mos-stage0 # the pinned beta cargo does the vendoring, same as x.py would
  ];

  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  GIT_SSL_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  CARGO_HTTP_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  # library manifests use unstable features (public-dependency); the beta
  # cargo needs this to even read them (same as x.py vendor does).
  RUSTC_BOOTSTRAP = "1";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall
    export HOME=$TMPDIR
    export CARGO_HOME=$TMPDIR/cargo-home

    git init -q rust-mos
    cd rust-mos
    git remote add origin https://github.com/${pins.rust-mos.owner}/${pins.rust-mos.repo}
    git -c protocol.version=2 fetch --depth 1 origin ${pins.rust-mos.rev}
    git checkout -q FETCH_HEAD

    # Only the submodules the build actually needs. src/llvm-project is NOT
    # fetched: LLVM comes from the Nix llvm-mos package (the upstream docker
    # build stubs the directory out the same way).
    git submodule update --init --depth 1 \
      src/tools/cargo library/stdarch library/backtrace

    # --- pin the moving git-branch reference (see header) -----------------
    count=$(grep -c 'branch="mos-0.1.148"' library/Cargo.toml || true)
    if [ "$count" -ne 1 ]; then
      echo "GUARD FAILED: expected exactly 1 mos-0.1.148 branch patch in library/Cargo.toml, found $count" >&2
      exit 1
    fi
    sed -i 's|branch="mos-0.1.148"|rev="${pins.compiler-builtins-148-rev}"|' library/Cargo.toml
    grep -q 'rev="${pins.compiler-builtins-148-rev}"' library/Cargo.toml

    # --- sync the library lockfile with the (rev-pinned) git patch --------
    cargo fetch --manifest-path library/Cargo.toml

    # --- vendor every workspace x.py will build ---------------------------
    # root workspace (compiler, rustdoc, in-tree tools) is the implicit
    # primary; --sync adds the separate workspaces.
    cargo vendor --versioned-dirs \
      --sync ./library/Cargo.toml \
      --sync ./src/tools/cargo/Cargo.toml \
      --sync ./src/bootstrap/Cargo.toml \
      vendor > vendor-config.toml
    mkdir -p .cargo
    cp vendor-config.toml .cargo/config.toml

    # x.py expects the directory to exist even though LLVM is external.
    mkdir -p src/llvm-project/libunwind

    # Strip all VCS state for a stable NAR hash (submodule .git are files).
    find . -name .git -prune -exec rm -rf {} +
    rm -rf $CARGO_HOME

    cd ..
    cp -r rust-mos $out
    runHook postInstall
  '';
}
