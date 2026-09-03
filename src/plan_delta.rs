//! What a plan changes, as the reader sees it.
//!
//! **One walk of the operation list, and two readings of it.** Which paths a
//! transition touches is asked by the preview, by the report after the fact,
//! by the deletion prompt and by the sweep of compiled shadows, and every one
//! of them has to get the same answer -- a `--pretend` that lists a file the
//! run then leaves alone is a second description of one transition. So the
//! walk is here, once, and the callers take the lines, the counts, or the
//! deletions out of the same value.
//!
//! Nothing here reads the filesystem: the plan carries both trees and every
//! before-image, so what changed is decided from the bundle alone.

/// Every path this bundle removes, managed tree entries included.
///
/// Shared with [`preview_lines`] so the sweep of compiled shadows cannot
/// disagree with the deletions the reader was shown.
pub(crate) fn deleted_paths(
    bundle: &jails_contracts::PlanBundle,
) -> Vec<jails_contracts::ProjectPath> {
    use jails_contracts::PlannedOperation as Op;
    let mut paths = Vec::new();
    for operation in &bundle.plan.operations {
        match operation {
            Op::PublishMergedTree { before, after, .. } => {
                let entries = |digest: &jails_contracts::ContentDigest| {
                    bundle
                        .trees
                        .get(digest)
                        .map(|tree| tree.entries.keys().cloned().collect())
                        .unwrap_or_default()
                };
                let was: std::collections::BTreeSet<_> =
                    before.as_ref().map(entries).unwrap_or_default();
                let now: std::collections::BTreeSet<_> = entries(after);
                paths.extend(was.difference(&now).cloned());
            }
            Op::RemoveReaderFile { path, .. } => paths.push(path.clone()),
            _ => {}
        }
    }
    paths
}

/// What a plan changes, as the reader's report and its counts.
///
/// **A file list is the change, not the tree.** Every path the managed tree
/// holds is in the plan's after-image whether or not this transition touches
/// it, so a `write` line per entry described a `entity field add` as
/// twenty-two files where `git status` showed three. `write` meant *in the
/// plan*; the reader read it as *rewritten*. The executor already skips an
/// entry whose bytes are already on disk, so the count under the list was
/// right and the list was wrong.
///
/// So a line is printed only where the before and after images differ, and
/// what the rest amount to is one number. The summary is counted off the
/// lines rather than beside them, which is what makes the count and the list
/// the same answer by construction.
/// One path this transition changes, and what it does to it.
///
/// **The value, not the sentence.** The human report renders these as
/// `  create  src/...` and `--output json` renders them as objects, and
/// making both readings of one list is what stops the two describing the
/// same transition differently -- which is the whole of I70.2.
pub(crate) struct Change {
    pub(crate) verb: &'static str,
    pub(crate) path: String,
}

pub(crate) struct Delta {
    /// Every path this transition changes, in the order it does it.
    pub(crate) changes: Vec<Change>,
    /// Managed paths the plan carries and this transition leaves alone.
    pub(crate) unchanged: usize,
}

impl Delta {
    /// One report line per change, in the order the executor does them.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.changes
            .iter()
            .map(|change| format!("  {:<8}{}", change.verb, change.path))
            .collect()
    }

    /// The one-line count under (or instead of) the list.
    pub(crate) fn summary(&self) -> String {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for (verb, noun) in [
            ("create", "created"),
            ("write", "written"),
            ("patch", "patched"),
            ("append", "appended"),
            ("delete", "deleted"),
        ] {
            let found = self
                .changes
                .iter()
                .filter(|change| change.verb == verb)
                .count();
            if found > 0 {
                counts.push((noun, found));
            }
        }
        if self.unchanged > 0 {
            counts.push(("unchanged", self.unchanged));
        }
        if counts.is_empty() {
            return "nothing to do".to_string();
        }
        counts
            .into_iter()
            .map(|(noun, count)| format!("{count} {noun}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The JDL this transition wrote into the model, as a reader would have
/// typed it.
///
/// **The CLI is sugar over one editable source, and this is where a reader
/// learns that.** `jails g record Money amount:long` is a shorthand for a
/// declaration; printing the declaration teaches the language from the tool
/// itself, and the next edit can be made by hand in the file. It is also the
/// honest answer to "what did that command actually change", because the
/// model is the only input the compiler reads.
///
/// Read out of the bundle rather than off the disk: `ReplaceModelFile`
/// carries the before- and after-images, so the hunk cannot disagree with
/// what apply will write, and `--pretend` shows the same lines without
/// having written them. Only the added lines: a removal names the
/// declaration it took out through the file list, and a reader looking at
/// `entity Money {` wants the thing they now have.
pub(crate) fn model_hunk(bundle: &jails_contracts::PlanBundle) -> Vec<String> {
    use jails_contracts::PlannedOperation as Op;
    let blob = |image: &jails_contracts::FileImageRef| -> Option<String> {
        bundle
            .blobs
            .get(&image.blob)
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
    };
    for operation in &bundle.plan.operations {
        let Op::ReplaceModelFile {
            path,
            before,
            after,
        } = operation
        else {
            continue;
        };
        let (Some(after), before) = (blob(after), before.as_ref().and_then(blob)) else {
            continue;
        };
        // Zero context: the declaration and nothing around it. `diff -u`'s
        // three lines of context would print a neighbouring entity a reader
        // did not just write.
        let Some(hunk) =
            jails_support::unified::diff(path.as_str(), before.as_deref(), Some(&after), 0)
        else {
            continue;
        };
        let mut added: Vec<String> = hunk
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .map(|line| line[1..].to_string())
            .collect();
        // The blank line the writer puts between declarations is part of the
        // edit and not part of the declaration, and a line of two spaces in
        // a report reads as a bug.
        while added.first().is_some_and(|line| line.trim().is_empty()) {
            added.remove(0);
        }
        while added.last().is_some_and(|line| line.trim().is_empty()) {
            added.pop();
        }
        return added.into_iter().map(|line| format!("  {line}")).collect();
    }
    Vec::new()
}

/// What this plan would do, one line per path it changes.
///
/// **A dry run that prints a count is not a dry run.** The question a reader
/// asks it is which of *their* files it is about to rewrite, and a digest and
/// an operation count answer neither. Verbs are the executor's own
/// distinctions rather than prose: a managed tree publishes, a reader file is
/// patched or removed, a migration is appended and can never be rewritten.
///
/// The managed tree expands to its files. It is one operation carrying a
/// whole after-image, so reporting it as `publish src` hides
/// exactly the thing that changed, and the tree manifest is already in the
/// bundle -- no filesystem read, and nothing here can disagree with what
/// apply will write.
pub(crate) fn preview_lines(bundle: &jails_contracts::PlanBundle) -> Vec<String> {
    preview(bundle).lines()
}

/// Which verb a single-file operation gets, or `None` when it changes nothing.
fn verb_for(
    before: Option<&jails_contracts::FileImageRef>,
    after: &jails_contracts::FileImageRef,
    existing: &'static str,
) -> Option<&'static str> {
    match before {
        None => Some("create"),
        Some(image) if image.blob == after.blob && image.mode == after.mode => None,
        Some(_) => Some(existing),
    }
}

/// The same walk, keeping what it left out.
pub(crate) fn preview(bundle: &jails_contracts::PlanBundle) -> Delta {
    use jails_contracts::PlannedOperation as Op;
    let mut changes: Vec<Change> = Vec::new();
    let mut unchanged = 0_usize;
    for operation in &bundle.plan.operations {
        match operation {
            Op::PublishMergedTree {
                root,
                before,
                after,
            } => {
                let entries = |digest: Option<&jails_contracts::ContentDigest>| {
                    digest
                        .and_then(|digest| bundle.trees.get(digest))
                        .map(|tree| tree.entries.clone())
                        .unwrap_or_default()
                };
                let was = entries(before.as_ref());
                let now = entries(Some(after));
                // Tree entries are already project-relative, so the root is
                // the operation's subject rather than a prefix to prepend.
                let _ = root;
                for path in was
                    .keys()
                    .chain(now.keys())
                    .collect::<std::collections::BTreeSet<_>>()
                {
                    let verb = match (was.get(path), now.get(path)) {
                        (None, _) => "create",
                        (Some(old), Some(new)) if old.blob == new.blob && old.mode == new.mode => {
                            unchanged += 1;
                            continue;
                        }
                        (Some(_), Some(_)) => "write",
                        (Some(_), None) => "delete",
                    };
                    changes.push(Change {
                        verb,
                        path: path.as_str().to_string(),
                    });
                }
            }
            Op::ReplaceModelFile {
                path,
                before,
                after,
            }
            | Op::ReplaceStateFile {
                path,
                before,
                after,
            } => match verb_for(before.as_ref(), after, "write") {
                None => unchanged += 1,
                Some(verb) => changes.push(Change {
                    verb,
                    path: path.as_str().to_string(),
                }),
            },
            Op::PatchReaderFile {
                path,
                before,
                after,
            } => match verb_for(before.as_ref(), after, "patch") {
                None => unchanged += 1,
                Some(verb) => changes.push(Change {
                    verb,
                    path: path.as_str().to_string(),
                }),
            },
            Op::RemoveReaderFile { path, .. } => {
                changes.push(Change {
                    verb: "delete",
                    path: path.as_str().to_string(),
                });
            }
            Op::AppendMigration { path, .. } => {
                changes.push(Change {
                    verb: "append",
                    path: path.as_str().to_string(),
                });
            }
        }
    }
    Delta { changes, unchanged }
}
