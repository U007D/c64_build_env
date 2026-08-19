# Xemu — LGB's emulator collection (github.com/lgblgblgb/xemu), built from
# source for the MEGA65 target only (`xmega65`), which backs `cargo xrun_mega65`.
#
# NOT the Xbox emulator: both nixpkgs and Homebrew ship a package called `xemu`
# that is xemu.app, an Original Xbox emulator. LGB's Xemu is unpackaged in both,
# which is why this builds it here rather than pulling it from nixpkgs.
#
# Only the mega65 target is built. The upstream default (`make all`) also builds
# c65, cvic20, clcd, ep128, primo and tvc — none of which this repo targets, and
# each costs build time.
{
  lib,
  stdenv,
  fetchFromGitHub,
  SDL2,
  pkg-config,
  python3,
}:

let
  # Commit date of `rev` below, in the YYYYMMDDHHMMSS form upstream stamps into
  # the binary. Pinned so the build embeds no wall-clock time.
  commitStamp = "20260129225930";
in
stdenv.mkDerivation rec {
  pname = "xemu-mega65";
  version = "unstable-2026-01-29";

  src = fetchFromGitHub {
    owner = "lgblgblgb";
    repo = "xemu";
    rev = "40dfef0d1d5f56be2469492715c12bdb32c75b67";
    hash = "sha256-ixKuD7GTHGD0+SDcXJZVXVAqgK8OCJJ7+v0hDX721sE=";
  };

  nativeBuildInputs = [ pkg-config python3 ];
  buildInputs = [ SDL2 ];

  # Upstream stamps build metadata into the binary by shelling out at link time:
  # `bash build/show-git-info` and `git log -1` (both want a .git, which
  # fetchFromGitHub does not export) plus `date`/`whoami`/`uname -n` (which would
  # make the output vary run to run). Replace them with the pinned revision so
  # the build neither depends on git nor embeds the build host/time.
  postPatch = ''
    substituteInPlace build/Makefile.common \
      --replace-fail '`bash $(TOPDIR)/build/show-git-info`' '${src.rev}' \
      --replace-fail '`whoami`@`uname -n` on `uname -s` `uname -r`' 'nix' \
      --replace-fail '`date`' '${version}' \
      --replace-fail \
        "@git log -1 --format=%cd --date=format:%Y%m%d%H%M%S > \$(TOPDIR)/build/objs/cdate.data || date '+%Y%m%d%H%M%S' > \$(TOPDIR)/build/objs/cdate.data" \
        '@echo ${commitStamp} > $(TOPDIR)/build/objs/cdate.data'
  '';

  # Build only the MEGA65 target, and name the goal explicitly.
  #
  # `do-all` is the real build goal. It cannot be omitted: the first target in
  # targets/mega65/Makefile is `recreatememcontent`, so that is make's default
  # goal — and it `git clone`s MEGA65/mega65-core to regenerate memcontent.c.
  # That is a network fetch (correctly refused by the Nix sandbox) and pointless
  # besides, since the generated memcontent.c/.h are committed upstream.
  #
  # RELEASE must be exactly `yes`; Makefile.common compares against that string,
  # so any other value silently yields a non-release build.
  buildPhase = ''
    runHook preBuild
    make -C targets/mega65 ARCH=native RELEASE=yes do-all -j$NIX_BUILD_CORES
    runHook postBuild
  '';

  # The per-target Makefile drops binaries in build/bin as <name>.<arch>;
  # install it under the plain name the cargo runner invokes.
  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin
    install -Dm755 build/bin/xmega65.native $out/bin/xmega65
    runHook postInstall
  '';

  meta = with lib; {
    description = "Xemu (LGB) — MEGA65 emulator, xmega65 target only";
    homepage = "https://github.com/lgblgblgb/xemu";
    license = licenses.gpl2Plus;
    mainProgram = "xmega65";
    platforms = platforms.unix;
  };
}
