# Stage0 bootstrap toolchain: the exact beta rustc/cargo/rust-std that
# rust-mos' src/stage0 pins (2025-02-18 = 1.85.0-beta), assembled into one
# prefix and fed to x.py via `[build] rustc/cargo`, so the sandboxed build
# never tries to download anything.
#
# These prebuilt binaries are a build-time-only input; nothing from them ends
# up in the installed rust-mos output. On Linux they are autoPatchelf'd so
# they can run inside the sandbox; on Darwin they run as-is.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  zlib,
  pins,
}:
let
  triple = stdenv.hostPlatform.rust.rustcTarget;
  s0 = pins.stage0;
  hashes =
    s0.sha256.${triple} or (throw "rust-mos: no stage0 hashes for ${triple}");
  component = name: channel: hash:
    fetchurl {
      url = "${s0.dist}/${s0.date}/${name}-${channel}-${triple}.tar.xz";
      sha256 = hash;
    };
  tarballs = [
    (component "rustc" "beta" hashes.rustc)
    (component "cargo" "beta" hashes.cargo)
    (component "rust-std" "beta" hashes.rust-std)
    # rust-mos' src/stage0 additionally pins a *nightly* rustfmt (same date);
    # x.py's bootstrap fetches it as part of stage0. Provide it here so the
    # sandboxed build never reaches for the network (fed via build.rustfmt).
    (component "rustfmt" "nightly" hashes.rustfmt)
  ];
in
stdenv.mkDerivation {
  pname = "rust-mos-stage0";
  version = "1.85.0-beta-${s0.date}";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    zlib
    stdenv.cc.cc.lib
  ];

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    ${lib.concatMapStringsSep "\n" (t: ''
      mkdir extract
      tar xf ${t} -C extract
      bash extract/*/install.sh --prefix=$out --disable-ldconfig
      rm -rf extract
    '') tarballs}
    # install.sh bookkeeping is not needed downstream
    rm -rf $out/lib/rustlib/{install.log,uninstall.sh,rust-installer-version,manifest-*,components}
    runHook postInstall
  '';

  # The bundled beta binaries are already stripped; don't let fixup touch
  # LLVM's bitcode-bearing rlibs.
  dontStrip = true;

  meta = {
    description = "Pinned Rust beta bootstrap toolchain for building rust-mos (build-time only)";
    # nixpkgs system doubles — NOT the rust target triples that key the hash
    # table above. meta.platforms is matched against hostPlatform.system/.config,
    # and Darwin's config is `arm64-apple-darwin`, not the rust
    # `aarch64-apple-darwin`; using the doubles avoids that spelling mismatch.
    platforms = [
      "aarch64-darwin"
      "x86_64-darwin"
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
}
