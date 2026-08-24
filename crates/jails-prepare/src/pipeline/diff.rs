//! Step 8: what the projection says the tree becomes, against what was there.
//!
//! One question per path, and the interesting half is the third answer. A path
//! nobody owns is compared; a path this change owns goes through §R5.3's
//! three-way rule instead, because "the generator changed it" and "the reader
//! changed it" are different facts that a two-way comparison collapses into
//! one -- and acting on the first when only the second is true overwrites
//! somebody's work.
//!
//! It also produces the `outputs` rows the ledger records. That is the same
//! subject: the base a future run measures an edit against is exactly the
//! bytes this run decided to write.

use super::*;

/// What the diff worked out, beyond the operations themselves.
pub(super) struct Diffed {
    pub(super) operations: Vec<FileOp>,
    pub(super) objects: ObjectBundle,
    /// The base and current images each managed output should record.
    pub(super) outputs: BTreeMap<ProjectPath, (StoredFileImage, LiveFileImage)>,
    /// Outputs whose file is going, so their row goes with it.
    pub(super) retired: BTreeSet<ProjectPath>,
    /// Paths where both sides changed the same lines.
    ///
    /// Carried out rather than raised, because a conflict is not an error:
    /// §R5.4 makes it a valid transition that commits marker bytes and one
    /// pending candidate. Erroring here would throw away the merge output the
    /// reader is supposed to resolve, and would refuse the *whole* transition
    /// over one file when every other path in it merged cleanly.
    pub(super) conflicts: Vec<Conflict>,
}

/// One path where both sides changed the same lines.
///
/// Deliberately only what a *report* needs. §R5.4's `PendingConflictPath`
/// carries far more -- both bases, the marker image, the tokens -- and is
/// frozen into an identity that includes an `InvocationFingerprint`. Nothing
/// builds one of those yet (`OperationIdentityV1.invocation` is `None` on
/// every route), so assembling a pending candidate now would mean inventing
/// the half of its identity that exists to prove a resumption is the same
/// request. That is worse than refusing: a conflict frozen under a made-up
/// fingerprint is one no resume can honestly match.
pub(super) struct Conflict {
    pub(super) path: ProjectPath,
    pub(super) hunks: usize,
}

/// Step 8: every path whose projected state differs from what was captured.
pub(super) fn diff(
    base: &ProjectSnapshot,
    projection: &ProjectedProject,
    rendered: &BTreeMap<ProjectPath, Vec<u8>>,
    prior: &BTreeMap<ProjectPath, crate::reconcile::PriorOutput>,
    previously_owned: &BTreeSet<ProjectPath>,
    read_object: &super::ObjectReader,
) -> Result<Diffed> {
    let mut operations = Vec::new();
    let mut objects: BTreeMap<ObjectId, Arc<[u8]>> = BTreeMap::new();
    let mut outputs: BTreeMap<ProjectPath, (StoredFileImage, LiveFileImage)> = BTreeMap::new();
    let mut retired: BTreeSet<ProjectPath> = BTreeSet::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    for (path, entry) in projection.overlay() {
        let before = base.read(path)?;
        let contributors = projection.contributors(path);
        let after: Option<(Vec<u8>, FileMode)> = match entry {
            ProjectedEntry::File(file) => Some((file.bytes.to_vec(), file.mode)),
            ProjectedEntry::Deferred { .. } => {
                let body = rendered.get(path).ok_or_else(|| {
                    format!("`{path}` is still a deferred render after materialisation")
                })?;
                Some((body.clone(), default_mode()))
            }
            ProjectedEntry::Deleted => None,
        };

        match (before, after) {
            (Captured::Absent, None) => {
                // A create that a later change deleted collapses to nothing,
                // which is §R3.2 step 8's rule and not an omission.
            }
            (Captured::Absent, Some((body, mode))) => {
                let object = intern(&mut objects, body);
                if !contributors.is_empty() {
                    record_output(&mut outputs, path, object, mode);
                }
                operations.push(FileOp::Create {
                    path: path.clone(),
                    after: object,
                    mode,
                    contributors,
                });
            }
            (Captured::Present(file), None) => {
                retired.insert(path.clone());
                operations.push(FileOp::Delete {
                    path: path.clone(),
                    before: GuardedImage {
                        object: ObjectRef::new(file.sha256, file.len),
                        mode: file.mode,
                    },
                    contributors,
                })
            }
            (Captured::Present(file), Some((body, mode))) => {
                let object = intern(&mut objects, body.clone());
                let live = ObjectRef::new(file.sha256, file.len);
                // Equal bytes *and* mode emit no operation. A file with the
                // right bytes and the wrong mode is not the file that was
                // meant, so mode is part of the comparison.
                if object.id == file.sha256 && mode == file.mode {
                    // The row still moves: a base that has caught up with what
                    // is on disk is what makes the *next* edit measurable.
                    if !contributors.is_empty() {
                        record_output(&mut outputs, path, object, mode);
                    }
                    continue;
                }
                // An *owned output* -- a file some entity claims, as opposed
                // to a shared file this change merely edits -- goes through
                // R5.3's reconciliation, which is where "jails did not write
                // this" is decided. Without this the preparation happily
                // planned a replace over a file somebody had written by hand,
                // and the receipt recorded the entity as its contributor.
                if !contributors.is_empty() {
                    let recorded = prior.get(path).copied();
                    if recorded.is_none() && previously_owned.contains(path) {
                        return Err(format!(
                            "`{path}` is jails' own output and its bytes differ from what this \
                             would write, but the store has not recorded the bytes jails wrote \
                             -- so it cannot tell your edits from a regeneration.\n       fix: \
                             destroy and regenerate, or keep the file. It was written before \
                             this jails recorded output bases."
                        ));
                    }
                    let live_image = FileImage::Present {
                        object: live,
                        mode: file.mode,
                    };
                    let desired = FileImage::Present { object, mode };
                    match crate::reconcile::reconcile(path, recorded, live_image, desired)? {
                        crate::reconcile::Decision::Refuse(why) => return Err(why),
                        // Nobody moved, or only the reader did. Either way no
                        // operation, and the base stays where it was -- which
                        // is what keeps the reader's edit an edit rather than
                        // becoming the new baseline.
                        crate::reconcile::Decision::Nothing => continue,
                        crate::reconcile::Decision::KeepUserBytes => {
                            keep_current(&mut outputs, path, prior, live, file.mode);
                            continue;
                        }
                        // Both sides moved to the same bytes. No write, but
                        // the base advances so the next edit measures from
                        // where the file actually is.
                        crate::reconcile::Decision::AdvanceBase { after, mode } => {
                            record_output(&mut outputs, path, after, mode);
                            continue;
                        }
                        // Both sides moved. §R5.3's fifth answer, and the
                        // only one that has to look at the text.
                        crate::reconcile::Decision::Merge { mode, .. } => {
                            let base_id = recorded.expect("a merge has a recorded base").base.id;
                            let base_bytes = objects
                                .get(&base_id)
                                .map(|bytes| bytes.to_vec())
                                .or_else(|| read_object(&base_id))
                                .ok_or_else(|| {
                                    format!(
                                        "`{path}` has a recorded base whose bytes the object \
                                         store does not hold, so a merge has nothing to measure \
                                         the two sides from.\n       fix: move your version \
                                         aside, or destroy and regenerate."
                                    )
                                })?;
                            match crate::merge::three_way(path, &base_bytes, &file.bytes, &body)? {
                                crate::merge::Merged::Clean(merged) => {
                                    // The merged bytes go on disk; the *base*
                                    // still advances to what the generator
                                    // wrote, so the reader's edit stays a
                                    // delta from the newest render rather than
                                    // becoming the baseline.
                                    let after = intern(&mut objects, merged);
                                    outputs.insert(
                                        path.clone(),
                                        (
                                            StoredFileImage { object, mode },
                                            LiveFileImage {
                                                sha256: after.id,
                                                len: after.len,
                                                mode,
                                            },
                                        ),
                                    );
                                    operations.push(FileOp::Replace {
                                        path: path.clone(),
                                        before: GuardedImage {
                                            object: live,
                                            mode: file.mode,
                                        },
                                        after,
                                        mode,
                                        contributors,
                                    });
                                    continue;
                                }
                                crate::merge::Merged::Conflicted {
                                    hunks,
                                    bytes,
                                    tokens,
                                } => {
                                    // Collected rather than raised. A merge
                                    // conflict is one path's problem, and
                                    // erroring here would refuse the whole
                                    // transition on the *first* one -- so a
                                    // reader fixes a file, runs again, and is
                                    // told about the next. Every conflicting
                                    // path is reported together.
                                    //
                                    // The marker bytes are produced and
                                    // dropped, which is the honest thing until
                                    // §R5.4's pending half exists: writing
                                    // them without a pending candidate would
                                    // record markers as the entity's output,
                                    // and the *next* generate would merge
                                    // against them.
                                    let _ = (bytes, tokens);
                                    conflicts.push(Conflict {
                                        path: path.clone(),
                                        hunks,
                                    });
                                    continue;
                                }
                            }
                        }
                        crate::reconcile::Decision::Create { .. }
                        | crate::reconcile::Decision::Replace { .. }
                        | crate::reconcile::Decision::Delete { .. } => {}
                    }
                    record_output(&mut outputs, path, object, mode);
                }
                operations.push(FileOp::Replace {
                    path: path.clone(),
                    before: GuardedImage {
                        object: live,
                        mode: file.mode,
                    },
                    after: object,
                    mode,
                    contributors,
                });
            }
        }
    }
    operations.sort_by(|a, b| a.target().cmp(b.target()));
    Ok(Diffed {
        operations,
        objects,
        outputs,
        retired,
        conflicts,
    })
}

/// The ordinary row: jails wrote these bytes, so they are both the base it
/// will measure the next change from and what is live.
pub(super) fn record_output(
    outputs: &mut BTreeMap<ProjectPath, (StoredFileImage, LiveFileImage)>,
    path: &ProjectPath,
    object: ObjectRef,
    mode: FileMode,
) {
    outputs.insert(
        path.clone(),
        (
            StoredFileImage { object, mode },
            LiveFileImage {
                sha256: object.id,
                len: object.len,
                mode,
            },
        ),
    );
}

/// The reader's bytes stand and the base does not move.
///
/// §R5.3: *"Their bytes stay; the recorded current image follows them, and the
/// base does not move."* Following the base as well would silently adopt the
/// edit as jails' own output, and the next generator change would then look
/// like a three-way merge against a baseline nobody rendered.
pub(super) fn keep_current(
    outputs: &mut BTreeMap<ProjectPath, (StoredFileImage, LiveFileImage)>,
    path: &ProjectPath,
    prior: &BTreeMap<ProjectPath, crate::reconcile::PriorOutput>,
    live: ObjectRef,
    mode: FileMode,
) {
    let Some(recorded) = prior.get(path) else {
        return;
    };
    outputs.insert(
        path.clone(),
        (
            StoredFileImage {
                object: recorded.base,
                mode: recorded.base_mode,
            },
            LiveFileImage {
                sha256: live.id,
                len: live.len,
                mode,
            },
        ),
    );
}
