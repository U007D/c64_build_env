//! `asm` — show the 6502 assembly the toolchain generates for the crate in the
//! current directory: the whole crate, or only functions whose symbol contains a
//! given substring. Normally invoked as `cargo xasm` (the dev shell exposes it on
//! PATH so it works from any crate directory; the `cargo xtask` alias is
//! repo-root-only, so it can't serve this).

use std::fs;
use std::process::{Command, ExitCode};

pub const HELP: &str = "\
cargo xasm --target <TARGET> [FUNCTION]         (also: cargo xtask asm)

Show the mos assembly for the crate in the current directory. Compiles it with
`--emit asm`, passing the mos target + build-std explicitly, then prints the
emitted `.s` — the whole crate, or, with FUNCTION, only the functions whose
symbol contains that text (substring match). Runs `cargo rustc`, so run it
inside `nix develop`, from a C64/MEGA65 crate.

targets:
  c64     Commodore 64 — 6502 assembly (mos-c64-none)
  mega65  MEGA65 — 45GS02 assembly (mos-mega65-none)

--target is required; there is no default, because the two machines compile to
different instruction sets. Shorthand aliases (repo root): cargo xasm_c64,
cargo xasm_mega65; plain `cargo xasm` is the same as cargo xasm_c64.
";

struct AsmOpts {
    /// Substring to match against function symbols; `None` prints the whole crate.
    function: Option<String>,
}

fn parse_asm_args(args: &[String]) -> Result<AsmOpts, String> {
    let mut function = None;
    for a in args {
        match a.as_str() {
            s if s.starts_with("--") => return Err(format!("unknown flag: {s}")),
            s if function.is_none() => function = Some(s.to_string()),
            s => return Err(format!("unexpected argument: {s}")),
        }
    }
    Ok(AsmOpts { function })
}

pub fn run(args: &[String]) -> ExitCode {
    // `--target` is pulled out first so the positional FUNCTION parser below
    // never sees it (it rejects anything starting with `--`).
    let (target, rest) = match crate::target::split_or_usage(args, "asm") {
        Ok(v) => v,
        Err(code) => return code,
    };
    match asm_inner(target, &rest) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR [asm]: {e}");
            ExitCode::FAILURE
        }
    }
}

fn asm_inner(target: crate::target::Target, args: &[String]) -> Result<(), String> {
    let opts = parse_asm_args(args)?;

    // Emit assembly for the crate in the current directory. The mos target +
    // build-std are passed explicitly (Target::cargo_flags), not inherited from
    // `.cargo/config.toml`, so this must run inside `nix develop`, from a
    // C64/MEGA65 crate. `--emit=asm=<path>` pins the output so we don't have to
    // know the target triple; `-Ccodegen-units=1` keeps the whole crate in that
    // one file. Both are rustc flags, so they go after `--` and apply only to
    // this crate. The temp filename carries the target so two concurrent runs
    // for different machines can't clobber each other.
    let out = std::env::temp_dir().join(format!(
        "cargo-xasm-{}-{}.s",
        target.name(),
        std::process::id()
    ));
    let emit = format!("--emit=asm={}", out.to_string_lossy());
    let status = Command::new("cargo")
        .args(["rustc", "--release"])
        .args(target.cargo_flags())
        .args(["--", "-Ccodegen-units=1", &emit])
        .status()
        .map_err(|e| {
            format!("failed to spawn `cargo`: {e} (run this inside `nix develop`, from a crate)")
        })?;
    if !status.success() {
        return Err("cross-compile failed (see cargo output above)".into());
    }

    let asm_text = fs::read_to_string(&out).map_err(|e| format!("reading {}: {e}", out.display()))?;
    let _ = fs::remove_file(&out);

    match &opts.function {
        None => print!("{asm_text}"),
        Some(query) => {
            let blocks = function_blocks(&asm_text, query);
            if blocks.is_empty() {
                let mut names = all_function_labels(&asm_text);
                names.sort();
                return Err(format!(
                    "no function symbol contains '{query}'\nfunctions found:\n  {}",
                    names.join("\n  "),
                ));
            }
            for b in blocks {
                print!("{b}");
            }
        }
    }
    Ok(())
}

/// A first-column function label line (`main:`, `_ZN…E:`) — a symbol
/// definition, not an indented instruction, a `.directive`, or a `.Llocal:`
/// branch target. Returns the symbol without the trailing `:`.
fn func_label(line: &str) -> Option<&str> {
    let l = line.trim_end();
    let first = l.chars().next()?;
    if first.is_whitespace() || first == '.' || !l.ends_with(':') {
        return None;
    }
    let label = &l[..l.len() - 1];
    if label.is_empty() || label.contains(char::is_whitespace) {
        return None;
    }
    Some(label)
}

/// Split the asm into (symbol, body) blocks, one per function. A block runs from
/// its label to its `.size <name>, …` end marker (LLVM emits one per function),
/// or to the next function label / EOF if none. Text between blocks (`.ident`,
/// `.section`, the next symbol's `.globl`) and the file preamble are dropped —
/// filtered output wants functions, and whole-crate output prints the raw file.
fn split_functions(asm: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut label: Option<String> = None;
    let mut body = String::new();
    for line in asm.lines() {
        if let Some(next) = func_label(line) {
            if let Some(prev) = label.take() {
                blocks.push((prev, std::mem::take(&mut body)));
            }
            label = Some(next.to_string());
        }
        if let Some(cur) = label.clone() {
            body.push_str(line);
            body.push('\n');
            if is_size_directive_for(line, &cur) {
                blocks.push((cur, std::mem::take(&mut body)));
                label = None;
            }
        }
    }
    if let Some(prev) = label.take() {
        blocks.push((prev, body));
    }
    blocks
}

/// True for a `.size <label>, …` directive naming `label` — LLVM's per-function
/// end marker.
fn is_size_directive_for(line: &str, label: &str) -> bool {
    line.trim_start()
        .strip_prefix(".size")
        .map(|rest| rest.trim_start().split([',', ' ', '\t']).next() == Some(label))
        .unwrap_or(false)
}

fn function_blocks(asm: &str, query: &str) -> Vec<String> {
    split_functions(asm)
        .into_iter()
        .filter(|(label, _)| label.contains(query))
        .map(|(_, body)| body)
        .collect()
}

fn all_function_labels(asm: &str) -> Vec<String> {
    split_functions(asm).into_iter().map(|(l, _)| l).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_asm_args() {
        let d = parse_asm_args(&[]).unwrap();
        assert!(d.function.is_none());

        let f = parse_asm_args(&["main".to_string()]).unwrap();
        assert_eq!(f.function.as_deref(), Some("main"));

        assert!(parse_asm_args(&["--bogus".to_string()]).is_err());
        assert!(parse_asm_args(&["a".to_string(), "b".to_string()]).is_err());
    }

    #[test]
    fn splits_and_filters_asm_functions() {
        // Preamble + two functions with `.size` end markers and inter-function
        // noise; `.globl`, `.Lloop:`, `.size`, and indented instructions must not
        // be taken for function labels, and a block must end at its `.size`.
        let asm = "\t.text\n.globl\tmain\nmain:\n\tlda #0\n.Lloop:\n\tjmp .Lloop\n\
                   .Lfunc_end0:\n\t.size\tmain, .Lfunc_end0-main\n\t.ident\t\"noise\"\n\
                   _ZN3foo3barE:\n\trts\n.Lfunc_end1:\n\t.size\t_ZN3foo3barE, .Lfunc_end1-_ZN3foo3barE\n\
                   \t.section\t\".note.GNU-stack\"\n";
        assert_eq!(
            all_function_labels(asm),
            vec!["main".to_string(), "_ZN3foo3barE".to_string()],
        );

        // `main` matches only `main`, keeps its local label, ends at `.size main`,
        // and does not sweep in the trailing `.ident`/next function.
        let m = function_blocks(asm, "main");
        assert_eq!(m.len(), 1);
        assert!(m[0].contains(".Lloop:"));
        assert!(m[0].contains(".size\tmain"));
        assert!(!m[0].contains(".ident"));
        assert!(!m[0].contains("_ZN3foo3barE"));

        let bar = function_blocks(asm, "bar");
        assert_eq!(bar.len(), 1);
        assert!(bar[0].contains("_ZN3foo3barE:"));
        assert!(bar[0].contains("rts"));
        assert!(!bar[0].contains(".section"));
        assert!(!bar[0].contains("main:"));

        assert!(function_blocks(asm, "nope").is_empty());
    }
}
