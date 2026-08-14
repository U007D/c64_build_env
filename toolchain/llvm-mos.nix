# llvm-mos: clang/lld/compiler-rt with the experimental MOS (6502) backend,
# built from mrk-its' fork at the exact commit the known-good rust-mos image
# used (branch cmp_fix).
#
# Configuration = upstream's clang/cmake/caches/MOS.cmake, plus the same
# deltas mrk-its'/mlund's rust-mos builds apply on top of it:
#   * LLVM_INSTALL_TOOLCHAIN_ONLY=OFF  (the cache sets ON; rustc needs
#     llvm-config, headers and libLLVM installed)
#   * LLVM_BUILD_LLVM_DYLIB / LLVM_LINK_LLVM_DYLIB = ON  (rustc is configured
#     with --enable-llvm-link-shared)
#   * host codegen backends re-enabled (the cache sets LLVM_TARGETS_TO_BUILD
#     to empty; rustc's stage1/stage2 must be able to emit host code)
{
  lib,
  stdenv,
  fetchFromGitHub,
  cmake,
  ninja,
  python3,
  pins,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "llvm-mos";
  version = "unstable-2025-03-09-${builtins.substring 0 9 pins.llvm-mos.rev}";

  src = fetchFromGitHub {
    inherit (pins.llvm-mos) owner repo rev hash;
  };

  nativeBuildInputs = [
    cmake
    ninja
    python3
  ];

  # cmake is invoked from $sourceRoot/build; the LLVM project root is llvm/.
  cmakeDir = "../llvm";
  cmakeBuildType = "MinSizeRel"; # keep the cache's build type

  cmakeFlags = [
    "-C../clang/cmake/caches/MOS.cmake"
    # rustc needs the full install, not the toolchain-only subset:
    "-DLLVM_INSTALL_TOOLCHAIN_ONLY=OFF"
    "-DLLVM_INSTALL_UTILS=ON"
    "-DLLVM_BUILD_LLVM_DYLIB=ON"
    "-DLLVM_LINK_LLVM_DYLIB=ON"
    # The cache leaves LLVM_TARGETS_TO_BUILD empty (MOS comes in via
    # LLVM_EXPERIMENTAL_TARGETS_TO_BUILD). Add both host arches we support so
    # one cache entry serves x86_64 and aarch64 hosts.
    "-DLLVM_TARGETS_TO_BUILD=X86;AArch64"
    # No tests in this build.
    "-DLLVM_INCLUDE_TESTS=OFF"
    "-DCLANG_INCLUDE_TESTS=OFF"
  ];

  # Default `ninja` + `ninja install` (NOT install-distribution): we want
  # llvm-config, LLVM/Clang libraries and headers installed alongside the
  # toolchain binaries.

  # LLVM's build can peak well over the default darwin file limits during
  # dylib links; keep parallel linking sane.
  enableParallelBuilding = true;

  requiredSystemFeatures = [ "big-parallel" ];

  passthru = { inherit (pins) llvm-mos; };

  meta = {
    description = "LLVM + Clang + lld with the MOS 6502 backend (mrk-its fork, rust-mos generation)";
    homepage = "https://github.com/mrk-its/llvm-mos";
    license = lib.licenses.ncsa;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
      "x86_64-darwin"
    ];
  };
})
