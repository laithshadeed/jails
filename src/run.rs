use crate::generate::find_project_root;
use crate::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    find_on_path_list(bin, std::env::split_paths(&paths))
}

fn find_on_path_list(bin: &str, dirs: impl Iterator<Item = PathBuf>) -> bool {
    dirs.into_iter().any(|dir| dir.join(bin).is_file())
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

fn run_inherited(mut cmd: Command, debug: bool) -> Result<()> {
    if debug {
        crate::debug_cmd(&cmd);
    }
    let program = cmd.get_program().to_string_lossy().to_string();
    let status = cmd.status().map_err(|e| format!("failed to run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

pub fn test(filter: Option<&str>, debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary());
    cmd.arg("test").current_dir(&root);
    if let Some(f) = filter {
        cmd.arg(format!("-Dtest={f}"));
    }
    run_inherited(cmd, debug)
}

pub fn build(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary());
    cmd.arg("package").current_dir(&root);
    run_inherited(cmd, debug)
}

/// Reformat in place. Spotless is a plugin, not a dependency, so an
/// unconfigured project fails with a Maven stack trace about an unknown
/// prefix -- checking first turns that into one actionable line.
pub fn fmt(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    require_spotless(&root)?;
    let mut cmd = Command::new(maven_binary());
    cmd.args(["spotless:apply"]).current_dir(&root);
    run_inherited(cmd, debug)
}

/// Reformat quietly, for `add format` to call the moment it installs the
/// plugin. A formatter has an opinion about line wrapping that no amount of
/// careful templating can predict, so the only way to leave the project
/// passing its own `verify` is to actually run it once.
///
/// Best-effort: a project without Maven on PATH is not a reason to fail the
/// capability, it just means the first `jails fmt` has work to do.
pub fn fmt_quietly(root: &std::path::Path) -> bool {
    Command::new(maven_binary())
        .args(["-q", "spotless:apply"])
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Everything the build has to say: format check, compile, tests. `verify`
/// rather than `test` because that is the phase `add format` binds to.
pub fn check(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary());
    cmd.arg("verify").current_dir(&root);
    run_inherited(cmd, debug)
}

fn require_spotless(root: &std::path::Path) -> Result<()> {
    let pom = fs::read_to_string(root.join("pom.xml")).map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if pom.contains("spotless-maven-plugin") {
        return Ok(());
    }
    Err("this project has no formatter configured -- run `jails add format` first".to_string())
}

/// Spawns `spring-boot:run` once and, on every change to a .java source
/// file, re-runs `mvn compile`. spring-boot-devtools (if on the
/// classpath) watches target/classes itself and restarts the already-
/// running JVM -- jails never kills/restarts the app process, just keeps
/// target/classes fresh. Without devtools this recompiles for nothing, so
/// that's checked upfront.
pub fn watch(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let pom = fs::read_to_string(root.join("pom.xml")).map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if !pom.contains("org.springframework.boot") {
        return Err("--watch only supports Spring Boot projects".to_string());
    }
    if !pom.contains("devtools") {
        eprintln!(
            "jails: spring-boot-devtools not found in pom.xml -- recompiles won't trigger a restart. Add it: jails new --deps web,devtools"
        );
    }

    let mut run_cmd = Command::new(maven_binary());
    run_cmd.arg("spring-boot:run").current_dir(&root);
    if debug {
        crate::debug_cmd(&run_cmd);
    }
    let mut child = run_cmd.spawn().map_err(|e| format!("failed to start spring-boot:run: {e}"))?;

    let src_root = root.join("src/main/java");
    let mut last_change = latest_mtime(&src_root);
    println!("jails: watching {} for changes (Ctrl-C to stop)", src_root.display());

    loop {
        std::thread::sleep(std::time::Duration::from_millis(750));

        if let Ok(Some(status)) = child.try_wait() {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("spring-boot:run exited with {status}"))
            };
        }

        let change = latest_mtime(&src_root);
        if change > last_change {
            last_change = change;
            println!("jails: change detected, recompiling...");
            let mut compile = Command::new(maven_binary());
            compile.arg("compile").current_dir(&root);
            if debug {
                crate::debug_cmd(&compile);
            }
            match compile.status() {
                Ok(s) if s.success() => println!("jails: recompiled -- devtools should restart shortly"),
                Ok(s) => eprintln!("jails: recompile failed ({s})"),
                Err(e) => eprintln!("jails: failed to run compile: {e}"),
            }
        }
    }
}

fn latest_mtime(dir: &Path) -> std::time::SystemTime {
    let mut latest = std::time::SystemTime::UNIX_EPOCH;
    let Ok(entries) = fs::read_dir(dir) else {
        return latest;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let sub = latest_mtime(&path);
            if sub > latest {
                latest = sub;
            }
        } else if path.extension().is_some_and(|ext| ext == "java") {
            if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
                if modified > latest {
                    latest = modified;
                }
            }
        }
    }
    latest
}

/// `args` is everything after `--`, forwarded verbatim to the program. A tool
/// that scaffolds CLI projects has to be able to *run* one with arguments, or
/// the edit loop drops out to raw `mvn` the moment the program takes input.
pub fn run(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let pom = fs::read_to_string(root.join("pom.xml")).map_err(|e| format!("failed to read pom.xml: {e}"))?;

    if pom.contains("org.springframework.boot") {
        if no_build {
            let jar = find_built_jar(&root)?;
            let mut run = Command::new("java");
            run.args(["-jar"]).arg(&jar).args(args).current_dir(&root);
            return run_inherited(run, debug);
        }
        let mut cmd = Command::new(maven_binary());
        cmd.arg("spring-boot:run").current_dir(&root);
        // spring-boot:run forks a JVM, so argv cannot simply be appended: the
        // plugin takes them as one space-joined property instead.
        if !args.is_empty() {
            cmd.arg(format!("-Dspring-boot.run.arguments={}", args.join(" ")));
        }
        return run_inherited(cmd, debug);
    }

    let (pkg, class_name) = find_main_class(&root)?;
    let fqcn = if pkg.is_empty() { class_name } else { format!("{pkg}.{class_name}") };

    if !no_build {
        let mut compile = Command::new(maven_binary());
        compile.arg("compile").current_dir(&root);
        run_inherited(compile, debug)?;
    } else if !root.join("target/classes").join(fqcn.replace('.', "/")).with_extension("class").is_file() {
        return Err(format!("target/classes has no compiled {fqcn} -- run `jails build` or `jails run` (without --no-build) first"));
    }

    let mut run = Command::new("java");
    run.args(["-cp", "target/classes", &fqcn]).args(args).current_dir(&root);
    run_inherited(run, debug)
}

/// Picks a jar out of target/ for --no-build's Spring Boot path. Excludes
/// spring-boot-maven-plugin's *.jar.original (its extension() is
/// "original", not "jar", so a plain "jar" filter already skips it).
fn find_built_jar(root: &Path) -> Result<PathBuf> {
    let target = root.join("target");
    let entries = fs::read_dir(&target).map_err(|_| {
        "no target/ directory -- run `jails build` or `jails run` (without --no-build) first".to_string()
    })?;
    entries
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "jar"))
        .ok_or_else(|| "no jar under target/ -- run `jails build` or `jails run` (without --no-build) first".to_string())
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

/// Prefer a jails CLI dispatcher when the project has one.
///
/// `generate cli` adds a second `static void main` to a project that already
/// has `App.java`, and picking whichever the directory walk reached first
/// would make `jails run` a coin toss -- usually landing on the Hello World
/// stub that ignores argv entirely. The dispatcher is the one that routes
/// arguments, so it wins.
fn search_main_file(dir: &Path) -> Option<PathBuf> {
    dispatcher_main_file(dir).or_else(|| any_main_file(dir))
}

fn dispatcher_main_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(nested) = dispatcher_main_file(&path) {
                found.push(nested);
            }
        } else if path.extension().is_some_and(|ext| ext == "java") {
            let dispatches = fs::read_to_string(&path)
                .map(|s| s.contains("static void main") && crate::generate::is_dispatcher(&s))
                .unwrap_or(false);
            if dispatches {
                found.push(path);
            }
        }
    }
    // More than one is not a preference jails can express for the user.
    found.sort();
    (found.len() == 1).then(|| found.remove(0))
}

fn any_main_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = any_main_file(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-run-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn latest_mtime_ignores_non_java_files_and_recurses() {
        let root = scratch("latest-mtime");
        let nested = root.join("com/example");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("notes.txt"), "x").unwrap();
        fs::write(nested.join("App.java"), "x").unwrap();

        let before_touch = latest_mtime(&root);

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(nested.join("App.java"), "changed").unwrap();
        let after_touch = latest_mtime(&root);
        assert!(after_touch > before_touch);

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(root.join("notes.txt"), "changed too").unwrap();
        let after_txt_touch = latest_mtime(&root);
        assert_eq!(after_txt_touch, after_touch, "non-.java changes shouldn't move the watermark");
    }

    #[test]
    fn find_on_path_list_finds_an_executable_in_one_of_the_dirs() {
        let dir = scratch("find-on-path");
        fs::write(dir.join("mvnd"), "").unwrap();
        let other = scratch("find-on-path-other");

        assert!(find_on_path_list("mvnd", [other.clone(), dir.clone()].into_iter()));
        assert!(!find_on_path_list("mvn", [other, dir].into_iter()));
    }

    #[test]
    fn find_main_class_extracts_package_and_class_name() {
        let root = scratch("main-class");
        let src = root.join("src/main/java/com/example/app");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("Cli.java"),
            "package com.example.app;\n\npublic class Cli {\n    public static void main(String[] args) {}\n}\n",
        )
        .unwrap();

        let (pkg, class_name) = find_main_class(&root).unwrap();
        assert_eq!(pkg, "com.example.app");
        assert_eq!(class_name, "Cli");
    }

    #[test]
    fn find_main_class_ignores_files_without_a_main_method() {
        let root = scratch("no-main-class");
        let src = root.join("src/main/java/com/example/app");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Helper.java"), "package com.example.app;\n\nclass Helper {}\n").unwrap();

        assert!(find_main_class(&root).is_err());
    }

    #[test]
    fn find_main_class_handles_default_package() {
        let root = scratch("default-package");
        let src = root.join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Cli.java"), "public class Cli {\n    public static void main(String[] args) {}\n}\n").unwrap();

        let (pkg, class_name) = find_main_class(&root).unwrap();
        assert_eq!(pkg, "");
        assert_eq!(class_name, "Cli");
    }
}
