# llvm-mos-sdk: 6502 platform libraries (crt0, linker scripts, per-target
# clang configs) built with the freshly built mos-clang.
#
# The SDK's CMake normally *downloads* a prebuilt llvm-mos compiler
# (cmake/bootstrap-compiler.cmake) when it can't find one. Passing
# -DLLVM_MOS=${llvm-mos} makes cmake/find-mos-compiler.cmake pick up our
# Nix-built compiler and skips every network path — required in the sandbox.
{
  lib,
  stdenv,
  fetchFromGitHub,
  cmake,
  ninja,
  symlinkJoin,
  llvm-mos,
  pins,
}:
let
  # llvm-mos ships libclang_rt.builtins.a with its symbol index stored as a
  # stray *regular* `/` member (a normal archive keeps `/` as metadata, never a
  # member). That non-standard layout breaks the SDK's `llvm-ar qL` merge step
  # (cmake/merge-libraries.cmake): the stray `/` corrupts the merged archive and
  # the index rebuild dies with "'/': The end of the file was unexpectedly
  # encountered". Normalize the runtime archives (drop the stray index member,
  # rebuild a proper one) in a lightweight symlink overlay so llvm-mos itself
  # isn't rebuilt — only the SDK's archive merges need this; rustc, which uses
  # the Rust compiler_builtins crate for MOS, never touches libclang_rt.
  llvm-mos-normalized = symlinkJoin {
    name = "llvm-mos-rt-normalized";
    paths = [ llvm-mos ];
    postBuild = ''
      for a in $(find "$out/lib/clang" -name 'libclang_rt.*.a'); do
        real=$(readlink -f "$a")
        rm "$a"
        cp "$real" "$a"
        chmod +w "$a"
        "${llvm-mos}/bin/llvm-ar" d "$a" / || true
        "${llvm-mos}/bin/llvm-ranlib" "$a"
        echo "normalized $a -> first member $(${llvm-mos}/bin/llvm-ar t "$a" | head -1)"
      done
    '';
  };
in
stdenv.mkDerivation {
  pname = "llvm-mos-sdk";
  version = "21-unstable-2025-01-01-${builtins.substring 0 7 pins.llvm-mos-sdk.rev}";

  src = fetchFromGitHub {
    inherit (pins.llvm-mos-sdk) owner repo rev hash;
  };

  nativeBuildInputs = [
    cmake
    ninja
  ];

  cmakeFlags = [
    # Use the normalized overlay so the `llvm-ar qL` merge sees a standard
    # runtime archive (see llvm-mos-normalized above).
    "-DLLVM_MOS=${llvm-mos-normalized}"
    "-DLLVM_MOS_BUILD_EXAMPLES=OFF"
    "-DLLVM_MOS_TEST_SUITE=OFF"
  ];

  # The SDK installs per-platform driver wrappers (mos-c64-clang, mos-nes-clang,
  # …) as relative symlinks to `mos-clang`, which lives in llvm-mos, not here.
  # Standalone they dangle by design — they resolve once this package is used
  # alongside llvm-mos (the combined toolchain in c64/check.nix / the devShell).
  # nixpkgs' default broken-symlink fixup check would otherwise fail the build.
  dontCheckForBrokenSymlinks = true;

  # The C64 target's link step emits a PRG image directly: link.ld places RAM
  # at 0x0801 and prepends SHORT(ORIGIN(ram)) — the 2-byte load address —
  # while basic-header.S provides the BASIC "SYS" stub. Nothing extra to do
  # here; consumers link with mos-c64-clang (installed by this package).

  passthru = { inherit (pins) llvm-mos-sdk; };

  meta = {
    description = "LLVM-MOS platform SDK (C64 and friends), built against the Nix llvm-mos";
    homepage = "https://github.com/llvm-mos/llvm-mos-sdk";
    license = lib.licenses.mit;
    platforms = llvm-mos.meta.platforms;
  };
}
