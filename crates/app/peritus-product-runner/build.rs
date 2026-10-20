//! Discover canonical skill packages and embed their resources without per-skill registration.

use std::{
    env,
    error::Error,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use yaml_rust2::YamlLoader;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = env::var_os("CARGO_MANIFEST_DIR").ok_or("missing CARGO_MANIFEST_DIR")?;
    let root = Path::new(&manifest).join("../../../skills");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut generated = String::from("pub(super) static SKILLS: &[BundledSkill] = &[\n");
    for entry in sorted_entries(&root)? {
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other(format!(
                "bundled skills must not contain symlinks: {}",
                entry.path().display()
            ))
            .into());
        }
        if !kind.is_dir() || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        let directory = entry.path();
        let name = entry.file_name().into_string().map_err(|_| "skill name is not UTF-8")?;
        let source = fs::read_to_string(directory.join("SKILL.md"))?;
        let description = description(&source).map_err(|error| {
            io::Error::other(format!("invalid skill metadata in {name}/SKILL.md: {error}"))
        })?;
        writeln!(
            generated,
            "BundledSkill {{ name: {name:?}, description: {description:?}, resources: &["
        )?;
        let mut files = Vec::new();
        resources(&directory, &directory, &mut files)?;
        files.sort_by(|left, right| left.0.cmp(&right.0));
        for (relative, absolute) in files {
            let absolute = absolute.to_str().ok_or("skill source path is not UTF-8")?;
            writeln!(
                generated,
                "BundledResource {{ path: {relative:?}, bytes: include_bytes!({absolute:?}) }},"
            )?;
        }
        generated.push_str("] },\n");
    }
    generated.push_str("];\n");
    let output = env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?;
    fs::write(Path::new(&output).join("bundled_skills.rs"), generated)?;
    Ok(())
}

fn sorted_entries(directory: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn resources(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), Box<dyn Error>> {
    for entry in sorted_entries(directory)? {
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            resources(root, &path, output)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)?
                .components()
                .map(|part| part.as_os_str().to_str().ok_or("skill resource path is not UTF-8"))
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            let absolute = fs::canonicalize(&path)?;
            output.push((relative, absolute));
        } else {
            return Err(io::Error::other(format!(
                "bundled skill resources must be regular files: {}",
                path.display()
            ))
            .into());
        }
    }
    Ok(())
}

fn description(source: &str) -> Result<String, Box<dyn Error>> {
    let mut lines = source.lines();
    if lines.next() == Some("---") {
        let mut metadata = String::new();
        let mut closed = false;
        for line in lines {
            if line == "---" {
                closed = true;
                break;
            }
            metadata.push_str(line);
            metadata.push('\n');
        }
        if !closed {
            return Err("unclosed YAML front matter".into());
        }
        let documents = YamlLoader::load_from_str(&metadata)?;
        if let Some(description) = documents.first().and_then(|doc| doc["description"].as_str()) {
            return Ok(description.trim().to_owned());
        }
    }
    Ok(source.lines().find_map(|line| line.strip_prefix("# ")).unwrap_or("").to_owned())
}
