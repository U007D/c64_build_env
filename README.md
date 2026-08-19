# `rust-mos-env`
A no-nonsense, let-me-get-to-it development environment for Rust targeting the Commodore 64 and 
other 6502-based targets.

# AI Policy: RDT/c
* AI was used for Research, Data and Tooling for this project.
* All source code from this crate present in the release binary was written by humans.

## Quick Start
1. `git clone https://github.com/u007d/rust-mos-env`
2. `cd rust-mos-env`
3. `nix develop` # or `nix develop -c <your preferred shell>`
4. Answer the Y/N questions.  You may download the toolchain binaries instead of building from
   scratch, depending on whether binary images for your platform have been made available.  As of
   the time of this writing, only Apple silicon images are available for download.
5. Wait while `nix` builds/downloads your 6502-specific compiler toolchain.
6. `cargo xinitenv` # initialize the build environment, install dependencies, emulator
7. `cd bin/hello_world && cargo xrun`
8. A Commodore 64 "Hello, 16-bit world, from Rust!" application will run and appear in an emulator.
9. Happy programming!

## Your projects: `bin/`

Every binary project is its own crate under `bin/`, and the repo root is the workspace that holds
them. The bundled example is `bin/hello_world`; to start a project of your own, copy it:

```sh
cp -r bin/hello_world bin/my_game     # then rename the package in bin/my_game/Cargo.toml
```

Run the build commands from inside a project to act on that project, or name the project from
anywhere else:

```sh
cd bin/hello_world && cargo xrun      # from this project's folder OR
cargo xrun --bin hello_world          # from anywhere in the repo
```

`nix develop` also works from anywhere in the repo: nix walks up to the flake at the root. (If
your nix is old enough to answer "does not contain a 'flake.nix'", run `nix develop` from the
root instead.)

**Your projects stay yours.** `.gitignore` ignores everything under `bin/` except `hello_world`,
so `rust-mos-env` never carries binary code but the example — your projects can live here, built
by the same commands, without appearing in `git status` or in a PR against this repo. Keep each
project in a repo of its own (a nested git repo under `bin/` is fine).

A project pins symbols to fixed addresses — a character set the VIC-II can see, sprite data,
music — through its own `memory.x` linker script, which `build.rs` hands to the linker. It is
optional, and `hello_world` has none: until something must live at a particular address, the
SDK's own layout is enough. Copy `bin/hello_world/memory.x.example` to `memory.x` in your project
when that day comes; the file documents both machines' memory maps.

The release and dev profiles live in the root `Cargo.toml` — cargo honours `[profile]` only in
the workspace root — and apply to every project.

**Moving an existing project in.** The glob `members = ["bin/*"]` picks up a new directory on its
own, so nothing in `rust-mos-env` needs editing. A project that used to be its own repo root does
carry files that were meaningful there and are not here; drop them as you move it in:

| Leftover | Why it goes |
| --- | --- |
| `[profile.*]` in `Cargo.toml` | Ignored in a member, and cargo prints a warning for it on *every* build of *any* project. The root's profiles apply instead. |
| `.cargo/config.toml` with `[build] target` / `[unstable] build-std` | Cargo merges configs from the cwd upward, so this leaks into anything run from inside the project — including `cargo xtask`, which must build for the host. The `cargo x*` commands pass the cross-compile flags explicitly, so it buys nothing. |
| `Cargo.lock` | Only the workspace root's lockfile is used. |
| `flake.nix` / `xtask/` / `targets/` | Superseded by this repo's. A nested `flake.nix` is otherwise harmless. |

Everything else — `src/`, `build.rs`, `memory.x`, `README.md`, its own `.git` — comes across
unchanged.

## Targets: `c64` and `mega65`

Every build command takes a machine suffix; the bare form is the C64.

```sh
cargo xbuild_c64 --release   # 6502   -> target/mos-c64-none/…      runs in VICE (x64sc)
cargo xbuild_mega65          # 45GS02 -> target/mos-mega65-none/…   runs in Xemu (xmega65)
cargo xrun_mega65            # build + launch Xemu
```

`_c64`, `_mega65`, and the bare default exist for `xbuild`, `xrun`, `xcheck`, and `xasm`. Each is
`cargo xtask <cmd> --target c64|mega65`; **`--target` is required** there, since the two machines
have different CPUs, linkers, and emulators. Calling `cargo xtask build` with no (or an unknown)
target prints the valid list.

The MEGA65 target is `targets/mos-mega65-none.json` — the dev shell appends the `targets/`
directory to `RUST_TARGET_PATH`. Nix flakes only see git-tracked files, so `mos-mega65-none.json`
must stay committed.

Fixed addresses in a project's `memory.x` are machine-specific — a MEGA65 PRG loads at `$2001`,
inside the range a C64 program can pin data at — so a project targeting both machines must place
sections where both maps agree. `bin/hello_world/memory.x.example` documents both maps.

### MEGA65 ROM

`xmega65` needs a C65 ROM to boot, and **`cargo xinitenv` installs it automatically**. Nothing to
do by hand unless you want a different ROM or that step reported an error.

It fetches Cloanto's free *C64 Forever* installer, extracts C65 ROM `910828`, and copies it to
Xemu's preferences directory as `MEGA65.ROM` — `~/Library/Application Support/xemu-lgb/mega65/`
on macOS, `~/.local/share/xemu-lgb/mega65/` on Linux. The ROM is Cloanto copyright: free to
download for your own use, **not redistributable**, so it never enters this repo or the Cachix
cache.

**That ROM boots, but SDK-built MEGA65 programs will not run on it.** It is a plain C65 — the
banner reads *"THE COMMODORE C64DX DEVELOPMENT SYSTEM, BASIC 10.0"* — and llvm-mos's MEGA65
startup assumes a MEGA65 memory environment, so programs crash immediately (Xemu reports
execution ending in zero page).

To actually run code you need the MEGA65 project's **enhanced closed-ROM** (v920422 or later),
made by patching the C65 ROM with a `.BDF` diff using M65Connect, both from
[files.mega65.org](https://files.mega65.org). It is licensed to MEGA65/C65 owners only, so
`xinitenv` does not attempt it — patch it yourself and copy the result over `MEGA65.ROM` at the
path above. `xinitenv` leaves an existing `MEGA65.ROM` alone, so it survives re-runs; delete it
to fall back to the Cloanto one. Verify with Xemu's log: `Closed-ROMs detected with version
920422` means you have the enhanced ROM, `910814` means you do not.

Note all documented C64 registers are defined in a Peripheral Access Crate (PAC)--a hierarchy of 
modules in the `c64_pac` crate (https://github.com/u007d/c64_pac), which `bin/hello_world` depends 
on.
The register/field names in the PAC are consistent with *Compute!'s Mapping the C64*.  An online 
text version of this book can be found at https://github.com/mist64/c64ref and a `.pdf` version can 
be found at https://archive.org/details/Compute_s_Mapping_the_64_and_64C.

## Background
A few years ago, I discovered [`rust_mos`](https://github.com/mrk-its/rust-mos) and was fascinated 
by the idea that I could run Rust on my first computer, a Commodore 64.  Much gnashing of teeth 
ensued as I build not only `rust-mos` but also the required `llvm-mos` and `llvm-mos-sdk` from 
source.

These projects would not build well on macOS, were sensitive to the exact commits of each of the 
projects and took several hours for each attempt.  I had a moment of success but was unable to get
a successful build thereafter for a few years.  The creator `mrk-its` does publish a `rust-mos`
container, but I don't really love containers and preferred a truly local build.

Fast-forward to this month when I finally gave in and used `podman` to set up an environment where
I could build for the C64 again in order to do a presentation to the Seattle Rust User Group (SRUG)
on the topic.  Afterward a SRUG member showed me his brilliant idea of using `nix` instead of 
`podman` to create the environment.

I loved his idea of using a deterministic build system--why hadn't I thought of that?? :)
So here it is.  This development environment is configured to build the latest (as of this writing)
version of `rust-mos`, deterministically, along with its dependencies, principally `llvm-mos` and
`llvm-mos-sdk`.

If cached binary images of the build tools are available, the environment will offer to pull them
down from `cachix`, rather than build locally--it's your choice.  The builds can be rather lengthy
depending on your hardware--from 20 mins to a few hours.  I have pushed Apple silicon images to
the cache, so if you are on macOS with M1+ hardware, you do not need to build from source.

Next, I compiled one of the demo apps I build for the SRUG presentation (it's a whopping 86 bytes,
compiled).  I was defining particular memory addresses as `const BORDER_COLOR: usize = 0xd020`,
and used these names to read the joystick and affect the screen to demonstrate control.  But I
wanted to do the same thing using proper, type-safe `embedded-hal` hardware registers.  So,
working with Claude, I carefully constructed a `c64.svd` file as if coming from a silicon 
vendor, defining all the documented registers and fields, giving them consistent naming, 
identifying when writing fields had side-effects that were surprising, and splitting such 
registers into read (`_R`) and write (`_W`) distinct types to minimize surprising behavior.

The `.svd` file weighs in at ~100KB of XML.  The tool `svd2rust` reads the `.svd` converting the
XML into typesafe register definitions for Rust known as a peripheral access crate or PAC, and the
`form` tool breaks up the ~500KB PAC file into a module hierarchy.  I've also published the PAC 
separately to `https://github.com/u007d/c64_pac` for separate use under permissive open source 
license.

You can see the PAC hierarchy and definitions in the `c64_pac` crate linked above.

When the naive demo program
```rust
<coming soon>
```
compiles to 86 bytes

is adapted to use the typesafe zero-runtime overhead (ZRO) abstractions of the PAC, I was curious
to find out if zero really meant zero.  Spoiler: Yes.  Yes, it does:
```rust
<coming soon>
```
compiles to... 153 bytes (!)

I can hear you thinking "Hold up, I thought you said all this abstraction was ZRO!!"

Let me explain.  While a 153-byte binary is tiny, it's bigger than an 86-byte binary.  So what's
happening?

# Zero Runtime Overhead
ZRO doesn't mean a feature takes zero bytes.  It means theres an abstraction (usually to make
the user experience easier, more readable, safer in terms of correctness guarantees or even all
of the above.  That's the case here.

It turns out that the PAC defines a `bool` as a static to ensure hardware resources can be
strictly owned, shared and mutated according to the normal rules of the borrow checker.  That bit
requires 1 byte of storage.  That byte of storage is called uninitialized storage (known as `.bss`
for block static storage or originally block storage start) which *must* be zeroed out before it
can be used--this is a guarantee provided by the C standard.  So Rust, which contrary to popular
belief, has a runtime--it's C's runtime (calld `crt0` or C runtime zero) which copies zeroes to all
`.bss` memory (of which we have only 1 byte).  This copy function, because it gets used, doesn't
get eliminated by the linker during compilation.  It's a one-time cost, but has nothing at all to
do with the PAC.  Use (`unsafe`) `Peripheral::steal()` instead of (safe) `Peripheral::take()` and
there's not static `bool`.  500KB PAC and all its abstractions in this demo code evaporate down 
to... you guess it: exactly the same 86 bytes.
