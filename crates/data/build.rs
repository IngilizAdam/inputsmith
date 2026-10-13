//! Embeds every `devices/*.json` and `art/*.svg` so a new device only needs its files.

use std::fmt::Write;
use std::path::Path;
use std::{env, fs};

fn list(dir: &Path, ext: &str) -> Vec<String> {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(ext))
        .collect();
    names.sort();
    names
}

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();
    let mut out = String::new();
    for (name, dir, ext, kind, macro_) in [
        ("DEVICE_FILES", "devices", ".json", "&str", "include_str"),
        ("ART_FILES", "art", ".svg", "&[u8]", "include_bytes"),
    ] {
        writeln!(out, "pub static {name}: &[(&str, {kind})] = &[").unwrap();
        for file in list(&root.join(dir), ext) {
            let path = root.join(dir).join(&file);
            writeln!(out, "    ({file:?}, {macro_}!({:?})),", path.display().to_string()).unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("embedded.rs"), out).unwrap();
}
