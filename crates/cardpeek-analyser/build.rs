//! Embeds all bundled Lua analysers in the generated resource table.

use std::{
    env,
    fmt::Write as _,
    fs,
    io::{Error, ErrorKind},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../analysers");
    println!("cargo:rerun-if-changed={}", source.display());
    let mut paths = fs::read_dir(&source)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file() && path.extension().is_some_and(|extension| extension == "lua")
    });
    paths.sort_by_key(|path| path.file_name().map(std::borrow::ToOwned::to_owned));

    let mut generated = String::from("const BUNDLED_ANALYSERS: &[(&str, &str)] = &[\n");
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "analyser filename is not UTF-8"))?;
        writeln!(
            generated,
            "    ({name:?}, include_str!({path:?})),",
            path = path.display().to_string(),
        )?;
    }
    generated.push_str("];\n");
    let output = PathBuf::from(env::var("OUT_DIR")?).join("bundled_analysers.rs");
    fs::write(output, generated)?;
    Ok(())
}
