# A usable MOS toolchain: llvm-mos's drivers/binutils plus per-platform driver
# scripts (mos-c64-clang, mos-a800xl-clang, …) that load the SDK's platform
# config explicitly.
#
# Why wrappers instead of the SDK's own symlinks: llvm-mos-sdk ships each
# mos-<platform>-clang as a symlink to mos-clang and relies on clang
# auto-loading mos-<platform>.cfg from the *installed* directory. In this split
# packaging clang's InstalledDir resolves to llvm-mos (immutable), where the SDK
# configs don't live, so auto-load silently fails and links can't find crt0 /
# the linker script. Passing `--config <cfg>` explicitly sidesteps InstalledDir;
# the cfg's own `<CFGDIR>` variable then locates the SDK's mos-platform/ libs
# relative to the config file. rustc's mos-*-none targets invoke these as the
# linker driver.
{
  lib,
  runCommand,
  llvm-mos,
  llvm-mos-sdk,
}:
runCommand "rust-mos-mos-toolchain"
  {
    passthru = { inherit llvm-mos llvm-mos-sdk; };
    meta = {
      description = "llvm-mos + llvm-mos-sdk combined into one usable MOS toolchain";
      platforms = llvm-mos.meta.platforms;
    };
  }
  ''
    mkdir -p $out/bin

    # Base drivers, lld and the llvm-* binutils from llvm-mos.
    for f in ${llvm-mos}/bin/*; do
      ln -s "$f" "$out/bin/$(basename "$f")"
    done

    # One driver script per SDK platform wrapper, loading its config explicitly.
    for w in ${llvm-mos-sdk}/bin/mos-*-clang; do
      plat=$(basename "$w")
      plat=''${plat%-clang}
      cfg=${llvm-mos-sdk}/bin/$plat.cfg
      [ -e "$cfg" ] || continue
      for s in clang clang++ clang-cpp; do
        dst=$out/bin/$plat-$s
        printf '#!/bin/sh\nexec %s/bin/mos-%s --config %s "$@"\n' \
          "${llvm-mos}" "$s" "$cfg" > "$dst"
        chmod +x "$dst"
      done
    done
  ''
