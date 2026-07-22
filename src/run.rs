use crate::generate::find_project_root;
use crate::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

/// Prefer mvnd when it's on PATH (matches the dotfiles' mvnd wrapper),
/// falling back to plain mvn.
fn maven_binary() -> &'static str {
    if find_on_path("mvnd") {
        "mvnd"
    } else {
        "mvn"
    }
}

fn run_inherited(mut cmd: Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().to_string();
    let status = cmd.status().map_err(|e| format!("failed to run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

pub fn test(filter: Option<&str>) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary());
    cmd.arg("test").current_dir(&root);
    if let Some(f) = filter {
        cmd.arg(format!("-Dtest={f}"));
    }
    run_inherited(cmd)
}

pub fn build() -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary());
    cmd.arg("package").current_dir(&root);
    run_inherited(cmd)
}

pub fn run() -> Result<()> {
    let root = find_project_root()?;
    let pom = fs::read_to_string(root.join("pom.xml")).map_err(|e| format!("failed to read pom.xml: {e}"))?;

    if pom.contains("org.springframework.boot") {
        let mut cmd = Command::new(maven_binary());
        cmd.arg("spring-boot:run").current_dir(&root);
        return run_inherited(cmd);
    }

    let (pkg, class_name) = find_main_class(&root)?;
    let fqcn = if pkg.is_empty() { class_name } else { format!("{pkg}.{class_name}") };

    let mut compile = Command::new(maven_binary());
    compile.arg("compile").current_dir(&root);
    run_inherited(compile)?;

    let mut run = Command::new("java");
    run.args(["-cp", "target/classes", &fqcn]).current_dir(&root);
    run_inherited(run)
}

/// Find the file with `static void main` under src/main/java and return
/// its (package, class name) so the caller can build the FQCN.
fn find_main_class(root: &Path) -> Result<(String, String)> {
    let src_root = root.join("src/main/java");
    let file = search_main_file(&src_root)
        .ok_or_else(|| "no file with `static void main` found under src/main/java".to_string())?;
    let contents = fs::read_to_string(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let pkg = contents
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("package ")?.trim().strip_suffix(';'))
        .unwrap_or("")
        .to_string();
    let class_name = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("could not determine class name for {}", file.display()))?;
    Ok((pkg, class_name))
}

fn search_main_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = search_main_file(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|ext| ext == "java") {
            if let Ok(contents) = fs::read_to_string(&path) {
                if contents.contains("static void main") {
                    return Some(path);
                }
            }
        }
    }
    None
}
