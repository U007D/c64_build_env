# Single source of truth for upstream revisions and fixed-output hashes.
#
# Provenance (verified 2026-07-24 against the repos' own CI config; see README):
#   Docker tag mrkits/rust-mos:13f2838f9-334fc98-8f3a80f8 decodes as
#   <llvm-mos 9>-<llvm-mos-sdk 7>-<rust-mos 8>, per
#   .github/workflows/build-rust-mos-image.yml at rust-mos 8f3a80f8.
#
# Hashes marked PREFETCH are placeholders (lib.fakeHash form). Fill them by
# running, once, from the repo root on a machine with network access:
#   cargo xtask prefetch-hashes
# (a zero-dependency Rust task in xtask/ that drives `nix build` and
# substitutes the reported hashes in place).
{
  rust-mos = {
    owner = "mrk-its";
    repo = "rust-mos";
    # tip of branch mos_rustc_1.87, "cmp fix", 2025-03-08; src/version = 1.87.0
    rev = "8f3a80f87cea2bd236d953027cb5bce9dfc63b89";
    version = "1.87.0-dev";
  };

  llvm-mos = {
    owner = "mrk-its"; # fork, NOT llvm-mos/llvm-mos: commit exists only on mrk-its' cmp_fix branch
    repo = "llvm-mos";
    # "add getActionDefinitionsBuilder({G_SCMP, G_UCMP})", 2025-03-09
    rev = "13f2838f909ca839ccb4d9a6c808de7b2ea60911";
    hash = "sha256-TKooBOlVdemjr8QLSx+4CqOIuYSWk56fI/uKJKB1G2U="; # PREFETCH:llvm-mos-source
  };

  llvm-mos-sdk = {
    owner = "llvm-mos"; # upstream
    repo = "llvm-mos-sdk";
    # "pce-mkcd: Skip PHDR sections which do not contain any data (BSS) (#387)",
    # 2025-01-01, v21.0.x era
    rev = "334fc9823745284938bfba543112f95bfb489b50";
    hash = "sha256-B2BCom2sFWeC+Vs8p4p18OGiZAKZK0UM7N16sP1Pc80="; # PREFETCH:llvm-mos-sdk-source
  };

  # mrk-its' compiler-builtins fork. Two branches are in play in this
  # generation (verified):
  #  - library/Cargo.toml [patch.crates-io] uses branch mos-0.1.148
  #    (tip 4505c7df..., "disable math", 2025-03-04) -- used when rustc's own
  #    std is built and when -Zbuild-std resolves through the rust-src patch.
  #  - the forked cargo (src/tools/cargo submodule, branch
  #    compiler_builtins_patched) injects branch mos-0.1.150
  #    (tip 090b8f3c...) into -Zbuild-std resolution via standard_lib.rs.
  # Both branch references are rewritten to these commit pins at source-prep
  # time so the vendored trees are reproducible even if the branches move.
  compiler-builtins-148-rev = "4505c7dff61454f8a18fe8147d7e29e57ee1ebd9";
  compiler-builtins-150-rev = "090b8f3cfdbf95afe7fda4fa2d9c99cc5ed1a69c";

  # Custom fixed-output derivations (source + cargo-vendor trees).
  rust-mos-src-hash = "sha256-On4Cx1/jW2k2td3WkcaiuSW58lklGIKh2Hl6/2OY7nA="; # PREFETCH:rust-mos-src
  check-vendor-hash = "sha256-0KIMgA4ggflOXDR9zy3sZuOfKDlaVOjnOpopsGQq3LE="; # PREFETCH:check-vendor

  # Stage0 bootstrap: pinned by rust-mos' own src/stage0 at 8f3a80f8
  # (compiler_date=2025-02-18, compiler_version=beta => 1.85.0-beta).
  # sha256 hex values below are copied verbatim from that src/stage0 file
  # (they are upstream's own pins, cross-checked over three fetches).
  stage0 = {
    date = "2025-02-18";
    dist = "https://static.rust-lang.org/dist";
    sha256 = {
      "x86_64-unknown-linux-gnu" = {
        rustc = "75a9d69d13e50bb22ec721f9c64d08282d76f285482b285bb61bacabeecd710c";
        cargo = "a8c569398d71cab0b90c809b1d869a2e3c5037407b5af804df08c205775981c2";
        rust-std = "04e31482a3ff18d8fdff40c4e95b22bc667bc0dd1ba06fadbe2479bae5d97288";
        rustfmt = "62748525678370dbda7f1cbd12a384e95d4af76103de998785278f6d1f076177";
      };
      "aarch64-unknown-linux-gnu" = {
        rustc = "330e217dbd1c507c8706aef5fbd0baf9584495445743f38668cdc962adfb125e";
        cargo = "02e52fc13ae52066031ac40dad039ad752b83582e5f9e74ecef206aa9206ac16";
        rust-std = "88170a13f9448b9671d252f0315aed94b6324716230db7307061d4890cfda70a";
        rustfmt = "e8112ae80c8fd43452612b79dabf471be561b7e828c9a2141c698429d761a49b";
      };
      "aarch64-apple-darwin" = {
        rustc = "e4b71f6456d9e62ada6909254606da7f6681f3da0f5bc7d2c3d5c387fea35743";
        cargo = "501feb9138f3c11a13fd649d6573924ead10f5e6fb1e759d880684bff11c12be";
        rust-std = "80043af05fb96c497bce55063f2733e37f243f85084f9aa60ab504d7c15c5cce";
        rustfmt = "0cd190bd5a064235f9cd6f6e7eae4725b3b53ae36493b3ea54b8f190372ba3ee";
      };
      "x86_64-apple-darwin" = {
        rustc = "bfd09bf8db9bbd7557df3f206207631cc4b10d59d0cf6b072e610646b5c7fa4a";
        cargo = "cbeff7c031b4ab4732207b90ee8849b27f03cb28d3e085bf4d70444347dbfc99";
        rust-std = "46bdd522559b61848cfcbc8c55d95b190a134f2a0ac19e3c7ebfaea867f9780b";
        rustfmt = "e085ee261acbaa41361a0263dd8f13644f8a4e3679eca6bb0851b03c647b80e8";
      };
    };
  };
}
