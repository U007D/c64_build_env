use std::{env, fs, path::PathBuf};

type Result<T, E = Box<dyn core::error::Error>> = core::result::Result<T, E>;

fn main() -> Result<()> {
    let out = PathBuf::from(env::var("OUT_DIR")?);

    fs::copy("memory.x", out.join("memory.x"))?;

    println!("cargo:rustc-link-search={}", out.display());

    println!("cargo:rustc-link-arg-bins=-Tmemory.x");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");

    Ok(())
}
