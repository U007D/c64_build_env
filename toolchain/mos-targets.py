#!/usr/bin/env python3
"""Generate mos-{sim,c64,a800xl}-none.json target specs.

Faithful replica of mrk-its/rust-mos create_mos_targets.py (branch
mos_target_latest), minus the rustup dependency: dump the built-in
mos-unknown-none spec from the freshly built rustc, then per platform set the
linker to the SDK's mos-<platform>-clang wrapper and the vendor field to the
platform name. The SDK wrapper supplies crt0, the BASIC SYS stub and the
PRG-emitting linker script for c64.

Usage: mos-targets.py <spec.json dumped from rustc> <dest-dir>
"""
import json
import os
import sys

PLATFORMS = ("sim", "c64", "a800xl")


def create_target(target_spec, platform):
    opts = target_spec.copy()
    opts.pop("is-builtin", None)
    opts["linker"] = f"mos-{platform}-clang"
    opts["vendor"] = platform
    return opts


if __name__ == "__main__":
    with open(sys.argv[1]) as fp:
        target_spec = json.load(fp)
    dest_dir = sys.argv[2]
    os.makedirs(dest_dir, exist_ok=True)
    for arch in PLATFORMS:
        target_file = os.path.join(dest_dir, f"mos-{arch}-none.json")
        with open(target_file, "w") as fp:
            json.dump(create_target(target_spec, arch), fp, indent=2)
        print("created", target_file)
