//! What to do when an applied intent has changed: regenerate, diff, merge.
//!
//! `plan.md` §11.1, and it is copier's shape rather than something invented
//! here -- with one thing cheaper: for the case this exists for, the
//! *generator* is unchanged and only the intent differs, so both sides can be
//! regenerated from the same binary. Generate the old intent and the new one
//! into scratch copies, `git diff --no-index` them, and `git merge-file` the
//! patch onto the project.
//!
//! **It needs a git repository**, and says so rather than falling back to
//! overwriting. Conflicts are left as `<<<<<<<` markers and counted, because
//! the alternative -- picking a side -- silently discards whichever half it did
//! not pick, and that is the reader's hand-edited code half the time.

use super::*;
use jails_support::apply;

/// A scratch tree that removes itself, so a failed merge leaves nothing behind.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Regenerate an applied intent on both sides and merge only the generator's
/// delta over the user's working tree. Every merge result is computed before
/// the first real file is written.
pub(super) fn reconcile_intent(
    root: &Path,
    previous: &ResolvedIntent,
    next: &ResolvedIntent,
) -> Result<usize> {
    if !std::process::Command::new("git")
        .args([
            "-C",
            root.to_string_lossy().as_ref(),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Err(
            "an applied manifest intent changed, but this project is not in a git repository.\n       \
             fix: run `git init`, commit or stash the current tree, then run `jails app apply` again."
                .to_string(),
        );
    }
    let kind = previous
        .kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string();
    let recorded =
        crate::generated_files::paths(root, &kind, &previous.name, previous.package.as_deref())?
            .ok_or_else(|| {
                format!(
                    "cannot update {} because its generated path record is missing.\n       \
             fix: restore `.jails/intents`, or destroy and re-apply this intent once.",
                    previous.label()
                )
            })?;

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "jails-intent-merge-{}-{unique}",
        std::process::id()
    )));
    let old_root = scratch.0.join("old");
    let new_root = scratch.0.join("new");
    copy_project(root, &old_root)?;
    copy_project(root, &new_root)?;
    for path in &recorded {
        let relative = path.strip_prefix(root).map_err(|_| {
            format!(
                "recorded generated path {} escapes the project",
                path.display()
            )
        })?;
        for copy in [&old_root, &new_root] {
            let path = copy.join(relative);
            if path.is_file() {
                fs::remove_file(&path)
                    .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
            }
        }
    }

    let old_project = crate::model::Project::load(&old_root)?;
    previous.apply_to(&old_project)?;
    let new_project = crate::model::Project::load(&new_root)?;
    next.apply_to(&new_project)?;

    let old_paths = intent_relative_paths(&old_root, previous)?;
    let new_paths = intent_relative_paths(&new_root, next)?;
    let mut paths = old_paths.union(&new_paths).cloned().collect::<Vec<_>>();
    paths.sort();

    let mut actions = Vec::new();
    let mut conflicts = 0;
    for relative in paths {
        let current = root.join(&relative);
        let old = old_root.join(&relative);
        let new = new_root.join(&relative);
        match (old.is_file(), new.is_file()) {
            (true, true) => {
                if fs::read(&old)
                    .map_err(|error| format!("failed to read {}: {error}", old.display()))?
                    == fs::read(&new)
                        .map_err(|error| format!("failed to read {}: {error}", new.display()))?
                {
                    continue;
                }
                if !current.is_file() {
                    return Err(format!(
                        "generated file {} was deleted locally while its intent changed.\n       \
                         fix: restore it or destroy the intent before applying the manifest.",
                        current.display()
                    ));
                }
                let output = std::process::Command::new("git")
                    .args(["merge-file", "-p", "--"])
                    .arg(&current)
                    .arg(&old)
                    .arg(&new)
                    .output()
                    .map_err(|error| {
                        format!(
                            "failed to run `git merge-file` for {}: {error}",
                            current.display()
                        )
                    })?;
                match output.status.code() {
                    Some(0) => {}
                    Some(1) => conflicts += 1,
                    _ => {
                        return Err(format!(
                            "git merge-file failed for {}: {}",
                            current.display(),
                            String::from_utf8_lossy(&output.stderr).trim()
                        ));
                    }
                }
                actions.push(MergeAction::Write(current, output.stdout));
            }
            (false, true) => {
                if current.exists() {
                    return Err(format!(
                        "updated intent would create {}, but it already exists.\n       \
                         fix: move the user-owned file or change the intent name/package.",
                        current.display()
                    ));
                }
                actions.push(MergeAction::Write(
                    current,
                    fs::read(&new)
                        .map_err(|error| format!("failed to read {}: {error}", new.display()))?,
                ));
            }
            (true, false) => {
                let current_bytes = fs::read(&current).map_err(|error| {
                    format!(
                        "failed to read generated file {}: {error}",
                        current.display()
                    )
                })?;
                let old_bytes = fs::read(&old)
                    .map_err(|error| format!("failed to read {}: {error}", old.display()))?;
                if current_bytes != old_bytes {
                    return Err(format!(
                        "updated intent removes {}, but that generated file was edited.\n       \
                         fix: preserve the edit elsewhere, then restore the generated version and retry.",
                        current.display()
                    ));
                }
                actions.push(MergeAction::Delete(current));
            }
            (false, false) => {}
        }
    }

    for action in actions {
        match action {
            MergeAction::Write(path, contents) => {
                apply::put_bytes(&path, contents)?;
                println!(
                    "  merge    {}",
                    path.strip_prefix(root).unwrap_or(&path).display()
                );
            }
            MergeAction::Delete(path) => {
                fs::remove_file(&path)
                    .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
                println!(
                    "  delete   {}",
                    path.strip_prefix(root).unwrap_or(&path).display()
                );
            }
        }
    }
    // The new rendering owns the same identity now, including any newly
    // introduced or removed path.
    let next_paths = new_paths
        .iter()
        .map(|relative| root.join(relative))
        .collect::<Vec<_>>();
    crate::generated_files::record(
        root,
        next.kind
            .to_possible_value()
            .expect("every ArtifactKind has a clap value")
            .get_name(),
        &next.name,
        next.package.as_deref(),
        &next_paths,
    )?;
    if matches!(next.kind, ArtifactKind::Record | ArtifactKind::Scaffold) && !next.fields.is_empty()
    {
        crate::generated_files::record_model(
            root,
            &next.name,
            next.package.as_deref(),
            &next.fields,
        )?;
    }
    Ok(conflicts)
}

pub(super) fn intent_relative_paths(
    root: &Path,
    intent: &ResolvedIntent,
) -> Result<std::collections::BTreeSet<PathBuf>> {
    let kind = intent
        .kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string();
    crate::generated_files::paths(root, &kind, &intent.name, intent.package.as_deref())?
        .ok_or_else(|| {
            format!(
                "isolated generation did not record paths for {}",
                intent.label()
            )
        })?
        .into_iter()
        .map(|path| {
            path.strip_prefix(root)
                .map(Path::to_path_buf)
                .map_err(|_| format!("generated path {} escapes its project", path.display()))
        })
        .collect()
}

pub(super) fn copy_project(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)
        .map_err(|error| format!("failed to create {}: {error}", to.display()))?;
    let entries = fs::read_dir(from)
        .map_err(|error| format!("failed to read {}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("failed to read an entry under {}: {error}", from.display())
        })?;
        let name = entry.file_name();
        if name == ".git" || name == "target" {
            continue;
        }
        let source = entry.path();
        let destination = to.join(&name);
        if source.is_dir() {
            copy_project(&source, &destination)?;
        } else if source.is_file() {
            fs::copy(&source, &destination).map_err(|error| {
                format!(
                    "failed to copy {} to {}: {error}",
                    source.display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}
