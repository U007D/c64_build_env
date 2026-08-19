# The C65 ROM that `xmega65` boots, extracted from Cloanto's freely downloadable
# C64 Forever installer.
#
# ############################################################################
# # LICENSING — READ BEFORE TOUCHING THIS FILE                               #
# ############################################################################
# The ROM is COPYRIGHTED (Cloanto holds the C65 ROM rights). Cloanto publishes
# the installer below for free download, and extracting a ROM from it for your
# own use is the route Xemu's own documentation recommends. Redistributing the
# extracted ROM is NOT permitted.
#
# Consequences:
#   * NEVER add this attr to CACHE_ATTRS in xtask/src/publish_toolchain_binaries.rs.
#     That would push the ROM to a public Cachix cache — i.e. redistribute it.
#   * Do not commit the extracted ROM to this repo.
# The derivation is local-only by design: it lands in your own /nix/store and is
# copied to your own Xemu preferences directory.
#
# ############################################################################
# # RE-PINNING when Cloanto retires this URL (e.g. ships v12)                #
# ############################################################################
# The fetch 404s rather than silently substituting something else. To re-pin:
#
#   1. Find the new installer URL — the version is in the filename:
#        https://cdn.cloanto.com/pub/c64forever/c64-forever-11-setup.msi
#   2. Hash it:
#        nix store prefetch-file --json <new-url>
#   3. Confirm the ROM's path inside the MSI (Cloanto may rename or move it):
#        nix shell nixpkgs#msitools -c msiextract -C out <installer>.msi
#        find out -iname 'c-65-*'
#
# Then update src.url, src.hash, and the `rom=` path in installPhase. The 128 KiB
# check below catches a wrong file. Expected ROM for reference:
#   sha256 0c4a00b45b65ca553b8a9f38cae83fe5f7dca7e809c24c0051ae40956640509d
#
# ############################################################################
# # WHY THIS ROM IS NOT PATCHED FOR THE MEGA65                               #
# ############################################################################
# This is the original C65 ROM. Xemu boots it — it reports "Closed-ROMs
# detected with version 910814" — but it is a plain C65, and SDK-built MEGA65
# programs CRASH on it: llvm-mos's mega65 startup assumes a MEGA65 memory
# environment. Running code needs the enhanced ROM (v920422+); this derivation
# only gets you a booting emulator.
#
# The MEGA65 project's *enhanced* closed-ROM (fixed BASIC, MEGA65-specific
# features) is a patched derivative of this ROM. Producing it requires a `.BDF`
# diff plus the M65Connect GUI from https://files.mega65.org, and per MEGA65's
# own documentation is licensed only to owners of MEGA65 hardware, a Dev Kit, or
# an original C65. There is no documented command-line path, and automating it
# would generate a licensed derivative work for users who may not be entitled to
# it — so initenv deliberately stops at the unpatched ROM. Owners can patch it
# themselves and drop the result in the Xemu pref dir; initenv will not overwrite
# an existing MEGA65.ROM.
{
  lib,
  stdenvNoCC,
  fetchurl,
  msitools,
}:

stdenvNoCC.mkDerivation {
  pname = "mega65-rom";
  version = "c65-910828";

  # Cloanto's free C64 Forever installer (~268 MB). Pinned by hash, so a silent
  # upstream replacement is a build failure, not a different ROM.
  src = fetchurl {
    url = "https://cdn.cloanto.com/pub/c64forever/c64-forever-11-setup.msi";
    hash = "sha256-Q6Es09xv7fUzo2Dhn1a62BYwMzqjtf54UX3gYbcwZSg=";
  };

  nativeBuildInputs = [ msitools ];

  # An .msi is not an archive stdenv knows how to unpack; msiextract does it in
  # buildPhase instead. (Extracting the embedded CAB by hand does not work —
  # the payload is only reachable through the MSI's own tables.)
  dontUnpack = true;

  buildPhase = ''
    runHook preBuild
    msiextract -C . "$src"
    runHook postBuild
  '';

  # Xemu looks for MEGA65.ROM in its preferences directory (targets/mega65/rom.c
  # loads "@MEGA65.ROM", where '@' means the pref dir), so install under that
  # name; initenv copies it there.
  #
  # 910828 is the C65 ROM revision Xemu's ROM tutorial specifies. A C65 ROM is
  # exactly 128 KiB — assert it, so a changed installer layout fails loudly
  # rather than installing something that merely exists.
  installPhase = ''
    runHook preInstall
    rom="Program Files/Cloanto/C64 Forever/Shared/rom/c-65-19910828.rom"
    test -f "$rom" || { echo "ROM not found at expected path: $rom" >&2; exit 1; }
    size=$(wc -c < "$rom")
    test "$size" -eq 131072 || { echo "unexpected ROM size $size (want 131072)" >&2; exit 1; }
    install -Dm444 "$rom" "$out/MEGA65.ROM"
    runHook postInstall
  '';

  meta = with lib; {
    description = "C65 ROM 910828 for Xemu's MEGA65 target, from Cloanto's C64 Forever";
    homepage = "https://www.c64forever.com/";
    # Proprietary: free to download, NOT free to redistribute. See the header.
    license = licenses.unfree;
    platforms = platforms.all;
  };
}
