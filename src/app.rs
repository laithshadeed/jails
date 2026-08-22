//! Apply a small, declarative application manifest through Jails' existing
//! capability and generator engines.
//!
//! This is deliberately domain-blind. A crawler and a support inbox are two
//! different lists of the same generic intents; neither gets a command,
//! branch, enum, or template in Jails core.

use crate::Result;
use crate::add::Capability;
use crate::generate::{self, ArtifactKind};
use clap::{Subcommand, ValueEnum};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MANIFEST: &str = ".jails/app.toml";
const STATE_FILE: &str = ".jails/app-state-v1";

#[derive(Subcommand)]
pub(crate) enum AppCommand {
    /// Create a documented starter manifest for this project
    Init {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Show the generic capability and generation intents without writing
    Plan {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Apply every unapplied intent, recording progress after each one
    Apply {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write Compose services but do not start them
        #[arg(long)]
        no_start: bool,
    },
}

#[derive(Debug, Default)]
struct Manifest {
    schema: u32,
    capabilities: Vec<Capability>,
}

#[derive(Debug, Default)]
struct GenerateIntent {
    kind: Option<ArtifactKind>,
    name: Option<String>,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
}

impl GenerateIntent {
    fn finish(self, number: usize) -> Result<ResolvedIntent> {
        let kind = self
            .kind
            .ok_or_else(|| format!("[[generate]] #{number} is missing `kind`"))?;
        let name = self
            .name
            .ok_or_else(|| format!("[[generate]] #{number} is missing `name`"))?;
        if name.is_empty() {
            return Err(format!("[[generate]] #{number} has an empty `name`"));
        }
        for value in self
            .fields
            .iter()
            .chain(self.indexes.iter())
            .chain(self.package.iter())
            .chain(self.strategy_on.iter())
            .chain(self.strategy_yields.iter())
        {
            if value.contains(['\n', '\r', '|']) {
                return Err(format!(
                    "[[generate]] #{number} contains a newline or `|`, which is not allowed"
                ));
            }
        }
        Ok(ResolvedIntent {
            kind,
            name,
            fields: self.fields,
            timestamps: self.timestamps,
            indexes: self.indexes,
            package: self.package,
            strategy_on: self.strategy_on,
            strategy_yields: self.strategy_yields,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedIntent {
    kind: ArtifactKind,
    name: String,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
}

impl ResolvedIntent {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name(),
            self.name,
            self.package.as_deref().unwrap_or(""),
            self.fields.join(","),
            self.timestamps,
            self.indexes.join(","),
            self.strategy_on.as_deref().unwrap_or(""),
            self.strategy_yields.as_deref().unwrap_or("")
        )
    }

    fn label(&self) -> String {
        format!(
            "generate {} {}{}",
            self.kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name(),
            self.name,
            if self.fields.is_empty() {
                String::new()
            } else {
                format!(" {}", self.fields.join(" "))
            }
        )
    }

    fn apply_to(&self, project: &crate::model::Project) -> Result<()> {
        generate::generate_in_project(
            project,
            self.kind,
            &self.name,
            &self.fields,
            self.timestamps,
            self.package.as_deref(),
            &self.indexes,
            self.strategy_on.as_deref(),
            self.strategy_yields.as_deref(),
            false,
        )
    }

    /// The recipe name as the ledger spells it -- clap's canonical value, never
    /// an alias, or one kind would be stored under two names.
    fn recipe(&self) -> String {
        self.kind
            .to_possible_value()
            .expect("every ArtifactKind has a clap value")
            .get_name()
            .to_string()
    }

    /// Whether a ledger row was built from this exact intent.
    fn is_recorded_as(&self, entry: &crate::ledger::Applied) -> bool {
        entry.fields == self.fields
            && entry.indexes == self.indexes
            && entry.on == self.strategy_on.clone().unwrap_or_default()
            && entry.yields == self.strategy_yields.clone().unwrap_or_default()
            && entry.timestamps == self.timestamps
    }

    /// Write this intent's spec onto a ledger row, leaving `files` -- which
    /// `generate` owns -- untouched.
    fn record_onto(&self, entry: &mut crate::ledger::Applied) {
        entry.fields = self.fields.clone();
        entry.indexes = self.indexes.clone();
        entry.on = self.strategy_on.clone().unwrap_or_default();
        entry.yields = self.strategy_yields.clone().unwrap_or_default();
        entry.timestamps = self.timestamps;
    }

    /// The intent a recorded row was built from.
    ///
    /// Fails only on a recipe name no current binary knows, which is a ledger
    /// written by a newer jails -- worth an error rather than a silent skip
    /// that would re-run the intent over its own files.
    fn from_applied(entry: &crate::ledger::Applied) -> Result<Self> {
        let kind = ArtifactKind::from_str(&entry.recipe, false)
            .map_err(|_| format!("ledger has unknown kind `{}`", entry.recipe))?;
        Ok(Self {
            kind,
            name: entry.name.clone(),
            package: (!entry.package.is_empty()).then(|| entry.package.clone()),
            fields: entry.fields.clone(),
            timestamps: entry.timestamps,
            indexes: entry.indexes.clone(),
            strategy_on: (!entry.on.is_empty()).then(|| entry.on.clone()),
            strategy_yields: (!entry.yields.is_empty()).then(|| entry.yields.clone()),
        })
    }

    fn decode(line: &str) -> Result<Self> {
        let parts = line.split('|').collect::<Vec<_>>();
        if parts.len() != 8 {
            return Err("stored intent has the wrong number of fields".to_string());
        }
        let kind = ArtifactKind::from_str(parts[0], false)
            .map_err(|_| format!("stored intent has unknown kind `{}`", parts[0]))?;
        let package = decode_text(parts[2])?;
        let strategy_on = decode_text(parts[6])?;
        let strategy_yields = decode_text(parts[7])?;
        Ok(Self {
            kind,
            name: decode_text(parts[1])?,
            package: (!package.is_empty()).then_some(package),
            timestamps: parts[3]
                .parse::<bool>()
                .map_err(|_| "stored intent has an invalid timestamps flag".to_string())?,
            fields: decode_list(parts[4])?,
            indexes: decode_list(parts[5])?,
            strategy_on: (!strategy_on.is_empty()).then_some(strategy_on),
            strategy_yields: (!strategy_yields.is_empty()).then_some(strategy_yields),
        })
    }

    fn from_legacy(root: &Path, line: &str) -> Result<Self> {
        let parts = line.split('|').collect::<Vec<_>>();
        if parts.len() != 8 {
            return Err("legacy stored intent has the wrong number of fields".to_string());
        }
        let kind = ArtifactKind::from_str(parts[0], false)
            .map_err(|_| format!("legacy stored intent has unknown kind `{}`", parts[0]))?;
        let package = (!parts[2].is_empty()).then_some(parts[2].to_string());
        // Record/scaffold fields have an unambiguous per-model record. The old
        // state key joined arrays with commas, which is ambiguous for
        // map<K,V>; prefer the recorded model whenever it exists.
        let fields = crate::generated_files::model_fields(root, parts[1], package.as_deref())?
            .unwrap_or_else(|| split_legacy_list(parts[3]));
        Ok(Self {
            kind,
            name: parts[1].to_string(),
            package,
            fields,
            timestamps: parts[4]
                .parse::<bool>()
                .map_err(|_| "legacy stored intent has an invalid timestamps flag".to_string())?,
            indexes: split_legacy_list(parts[5]),
            strategy_on: (!parts[6].is_empty()).then_some(parts[6].to_string()),
            strategy_yields: (!parts[7].is_empty()).then_some(parts[7].to_string()),
        })
    }
}

/// What the project already has, read from the one ledger.
///
/// This used to be `.jails/app-state-v1`, a second registry of the same
/// entities `generate` was already recording paths for under the same
/// `(recipe, name, package)` key -- `abstract.md` §6.3's specimen of one fact
/// kept in two places. The columns differ (`app apply` owns the spec, `generate`
/// owns the file list) but the row does not, so they are one row now.
struct AppState {
    ledger: crate::ledger::Ledger,
}

impl AppState {
    fn entry(&self, intent: &ResolvedIntent) -> Option<&crate::ledger::Applied> {
        let recipe = intent.recipe();
        self.ledger
            .applied
            .iter()
            .find(|entry| entry.is(&recipe, &intent.name, intent.package.as_deref()))
    }

    fn is_applied(&self, intent: &ResolvedIntent) -> bool {
        self.entry(intent)
            .is_some_and(|entry| intent.is_recorded_as(entry))
    }

    /// The intent this one replaces, when the manifest has been edited.
    ///
    /// A row with files but **no** recorded spec is not an emptied manifest
    /// entry -- it is an artifact `generate` wrote directly, which never had a
    /// spec to compare against. Reading it as a change would hand the reader a
    /// three-way merge against a spec nobody ever wrote.
    fn previous(&self, intent: &ResolvedIntent) -> Result<Option<ResolvedIntent>> {
        match self.entry(intent) {
            Some(entry) if entry.has_spec() && !intent.is_recorded_as(entry) => {
                ResolvedIntent::from_applied(entry).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Record an applied intent, against the ledger **as it is on disk now**.
    ///
    /// Not against the copy this struct was built from: `apply_to` ran
    /// `generate` in between, and generate recorded the paths it wrote onto the
    /// same row. Writing back a pre-generate snapshot would erase them.
    fn record(&mut self, root: &Path, intent: &ResolvedIntent) -> Result<()> {
        let mut current = crate::ledger::load(root)?;
        current.version = env!("CARGO_PKG_VERSION").to_string();
        intent.record_onto(crate::ledger::entry_mut(
            &mut current,
            &intent.recipe(),
            &intent.name,
            intent.package.as_deref(),
        ));
        crate::ledger::save(root, &current)?;
        self.ledger = current;
        Ok(())
    }
}

pub(crate) fn run(command: AppCommand, debug: bool, pretend: bool) -> Result<()> {
    let root = generate::find_project_root()?;
    match command {
        AppCommand::Init { manifest } => init(&root, manifest.as_deref(), pretend),
        AppCommand::Plan { manifest } => plan(&root, manifest.as_deref()),
        AppCommand::Apply { manifest, no_start } if pretend => {
            let _ = no_start;
            plan(&root, manifest.as_deref())
        }
        AppCommand::Apply { manifest, no_start } => {
            apply(&root, manifest.as_deref(), no_start, debug)
        }
    }
}

fn init(root: &Path, requested: Option<&Path>, pretend: bool) -> Result<()> {
    let path = match requested {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(DEFAULT_MANIFEST),
    };
    if path.exists() {
        return Err(format!(
            "application manifest already exists: {}.\n       fix: edit it, or pass --manifest with a new path.",
            path.display()
        ));
    }
    if pretend {
        println!("would create application manifest {}", path.display());
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    crate::apply::put(
        &path,
        "# Generic application intent. Add capabilities, then one [[generate]] table per slice.\n\
         schema = 1\n\
         capabilities = []\n\n\
         # [[generate]]\n\
         # kind = \"scaffold\"\n\
         # name = \"Note\"\n\
         # fields = [\"id:uuid@pk\", \"title:string!\"]\n\
         # timestamps = true\n",
    )?;
    println!("created application manifest {}", path.display());
    Ok(())
}

fn plan(root: &Path, requested: Option<&Path>) -> Result<()> {
    let path = manifest_path(root, requested)?;
    let (manifest, intents) = read_manifest(&path)?;
    crate::add::preflight_in(&project_at(root)?, &manifest.capabilities, None, None)?;
    let state = read_state(root)?;

    println!("application plan: {}", path.display());
    println!("schema: {}", manifest.schema);
    for capability in &manifest.capabilities {
        println!("  ensure capability  {}", capability.label());
    }
    for intent in &intents {
        let status = if state.is_applied(intent) {
            "applied"
        } else if state.previous(intent)?.is_some() {
            "update"
        } else {
            "pending"
        };
        println!("  {status:7}  {}", intent.label());
    }
    println!();
    println!("plan only -- nothing was written");
    Ok(())
}

/// Apply a manifest against a project root the caller already knows.
///
/// `run` finds the root from the process CWD, which is right for `jails app
/// apply` and wrong for `jails new --app`: the project it should apply to is
/// the one that command just created, not whatever encloses the directory the
/// user is standing in.
pub(crate) fn apply_in(root: &Path, no_start: bool, debug: bool) -> Result<()> {
    apply(root, None, no_start, debug)
}

/// The project this manifest applies to, resolved from the root `apply` was
/// given rather than from the process CWD.
///
/// Deliberately re-read per step rather than resolved once and threaded.
/// `Project` is a *snapshot*: applying a capability rewrites `pom.xml` and
/// `jails.toml`, so the next capability has to plan against the project as it
/// now is. Caching it here would hand step N+1 the flavour, dependency set and
/// capability list from before step N ran -- which is the same staleness bug
/// as reading a pom into a variable and splicing against the copy.
fn project_at(root: &Path) -> Result<crate::model::Project> {
    crate::model::Project::load(root)
}

fn apply(root: &Path, requested: Option<&Path>, no_start: bool, debug: bool) -> Result<()> {
    let path = manifest_path(root, requested)?;
    let (manifest, intents) = read_manifest(&path)?;
    crate::add::preflight_in(&project_at(root)?, &manifest.capabilities, None, None)?;

    println!("applying application manifest {}", path.display());
    for &capability in &manifest.capabilities {
        // Formatting only has useful work after generation. Installing it
        // here used to run Spotless once over the starter project and then a
        // second time during reconciliation over the generated sources. The
        // latter is the actual invariant; defer installation so one Maven
        // lifecycle formats the complete final tree.
        if matches!(capability, Capability::Format) {
            continue;
        }
        crate::add::add_in(
            &project_at(root)?,
            capability,
            None,
            false,
            None,
            debug,
            no_start,
        )?;
    }

    let mut state = read_state(root)?;
    for intent in intents {
        if state.is_applied(&intent) {
            println!("  applied  {}", intent.label());
            continue;
        }
        let conflicts = if let Some(previous) = state.previous(&intent)? {
            reconcile_intent(root, &previous, &intent)?
        } else {
            // Against the project this manifest belongs to, not whatever
            // encloses the process CWD -- which is a different directory when
            // `jails new --app` applies to the project it just created.
            intent.apply_to(&project_at(root)?)?;
            0
        };
        state.record(root, &intent)?;
        if conflicts > 0 {
            return Err(format!(
                "updated intent left conflict markers in {conflicts} file(s).\n       \
                 fix: resolve the <<<<<<< / ======= / >>>>>>> blocks, then run `jails check`."
            ));
        }
    }

    // A generator can create a new integration point for an already-applied
    // capability. The database capability is the concrete first case: it
    // wires every existing @SpringBootTest to Testcontainers, then a later
    // generator creates more @SpringBootTest classes. A second idempotent
    // reconciliation makes capability invariants describe the final tree,
    // not only the tree that happened to exist at installation time. Format
    // is deliberately installed here for the first time (see above).
    if !manifest.capabilities.is_empty() {
        println!("reconciling capabilities against generated artifacts");
    }
    for capability in manifest.capabilities {
        crate::add::add_in(
            &project_at(root)?,
            capability,
            None,
            false,
            None,
            debug,
            true,
        )?;
    }

    println!("application manifest applied");
    Ok(())
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

enum MergeAction {
    Write(PathBuf, Vec<u8>),
    Delete(PathBuf),
}

/// Regenerate an applied intent on both sides and merge only the generator's
/// delta over the user's working tree. Every merge result is computed before
/// the first real file is written.
fn reconcile_intent(
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
                crate::apply::put_bytes(&path, contents)?;
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

fn intent_relative_paths(
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

fn copy_project(from: &Path, to: &Path) -> Result<()> {
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

fn manifest_path(root: &Path, requested: Option<&Path>) -> Result<PathBuf> {
    let path = match requested {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()
            .map_err(|e| format!("failed to get cwd: {e}"))?
            .join(path),
        None => root.join(DEFAULT_MANIFEST),
    };
    if !path.is_file() {
        return Err(format!(
            "application manifest not found: {}\n\nfix: create {DEFAULT_MANIFEST}, or pass `--manifest <path>`.",
            path.display()
        ));
    }
    Ok(path)
}

fn read_manifest(path: &Path) -> Result<(Manifest, Vec<ResolvedIntent>)> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    parse_manifest(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_manifest(text: &str) -> Result<(Manifest, Vec<ResolvedIntent>)> {
    let mut manifest = Manifest::default();
    let mut current: Option<GenerateIntent> = None;
    let mut resolved = Vec::new();

    for (offset, raw) in text.lines().enumerate() {
        let line_number = offset + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[generate]]" {
            if let Some(intent) = current.take() {
                resolved.push(intent.finish(resolved.len() + 1)?);
            }
            current = Some(GenerateIntent::default());
            continue;
        }
        if line.starts_with('[') {
            return Err(format!(
                "line {line_number}: unknown table `{line}`; only `[[generate]]` is supported"
            ));
        }
        let (key, raw_value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {line_number}: expected `key = value`, found `{line}`"))?;
        let key = key.trim();
        let value = raw_value.trim();

        if let Some(intent) = current.as_mut() {
            match key {
                "kind" => {
                    let value = string(value, line_number, key)?;
                    intent.kind = Some(ArtifactKind::from_str(value, false).map_err(|_| {
                        format!(
                            "line {line_number}: unknown generator kind `{value}`; known: {}",
                            ArtifactKind::value_variants()
                                .iter()
                                .filter_map(|kind| kind.to_possible_value())
                                .map(|value| value.get_name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?);
                }
                "name" => intent.name = Some(string(value, line_number, key)?.to_string()),
                "fields" => intent.fields = string_array(value, line_number, key)?,
                "timestamps" => {
                    intent.timestamps = match value {
                        "true" => true,
                        "false" => false,
                        _ => {
                            return Err(format!(
                                "line {line_number}: `timestamps` must be true or false"
                            ));
                        }
                    }
                }
                "indexes" => intent.indexes = string_array(value, line_number, key)?,
                "package" => intent.package = Some(string(value, line_number, key)?.to_string()),
                // `on` and `yields` are the names. `strategy_on` and
                // `strategy_yields` are kept as deprecated aliases because
                // they shipped in a user-facing file format, and a manifest
                // people already wrote must keep working.
                //
                // The old spelling is a naming failure worth not repeating:
                // the flag was invented for `g strategy` and then reused by
                // `usecase`, `query`, `transition`, `durable-job` and
                // `command`, so a manifest ends up saying `strategy_on` on an
                // intent that is not a strategy. abstract.md §4.4 is the
                // diagnosis -- an implementation detail of the first case
                // became schema, and the word in the file stopped naming the
                // thing.
                "on" | "strategy_on" => {
                    if intent.strategy_on.is_some() {
                        return Err(format!(
                            "line {line_number}: `on` is already set for this intent; \
                             `strategy_on` is a deprecated alias for it, so pass one or the other"
                        ));
                    }
                    intent.strategy_on = Some(string(value, line_number, key)?.to_string())
                }
                "yields" | "strategy_yields" => {
                    if intent.strategy_yields.is_some() {
                        return Err(format!(
                            "line {line_number}: `yields` is already set for this intent; \
                             `strategy_yields` is a deprecated alias for it, so pass one or the other"
                        ));
                    }
                    intent.strategy_yields = Some(string(value, line_number, key)?.to_string())
                }
                _ => {
                    return Err(format!(
                        "line {line_number}: unknown [[generate]] key `{key}`; known: \
                         kind, name, fields, timestamps, indexes, package, on, yields"
                    ));
                }
            }
            continue;
        }

        match key {
            "schema" => {
                manifest.schema = value
                    .parse::<u32>()
                    .map_err(|_| format!("line {line_number}: `schema` must be an integer"))?;
            }
            "capabilities" => {
                for label in string_array(value, line_number, key)? {
                    let capability = Capability::from_str(&label, false).map_err(|_| {
                        format!(
                            "line {line_number}: unknown capability `{label}`; known: {}",
                            Capability::value_variants()
                                .iter()
                                .map(|capability| capability.label())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                    if !manifest.capabilities.contains(&capability) {
                        manifest.capabilities.push(capability);
                    }
                }
            }
            _ => return Err(format!("line {line_number}: unknown top-level key `{key}`")),
        }
    }

    if let Some(intent) = current {
        resolved.push(intent.finish(resolved.len() + 1)?);
    }
    if manifest.schema != 1 {
        return Err(format!(
            "unsupported schema {}; this Jails release supports schema 1",
            manifest.schema
        ));
    }
    let mut keys = HashSet::new();
    for intent in &resolved {
        if !keys.insert(intent.key()) {
            return Err(format!("duplicate intent: {}", intent.label()));
        }
    }
    Ok((manifest, resolved))
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn string<'a>(value: &'a str, line: usize, key: &str) -> Result<&'a str> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("line {line}: `{key}` must be a double-quoted string"))
}

fn string_array(value: &str, line: usize, key: &str) -> Result<Vec<String>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| format!("line {line}: `{key}` must be a one-line string array"))?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let bytes = inner.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() || bytes[at] != b'"' {
            return Err(format!(
                "line {line}: `{key}` must contain only double-quoted strings"
            ));
        }
        at += 1;
        let start = at;
        let mut escaped = false;
        while at < bytes.len() {
            if escaped {
                escaped = false;
                at += 1;
                continue;
            }
            match bytes[at] {
                b'\\' => escaped = true,
                b'"' => break,
                _ => {}
            }
            at += 1;
        }
        if at == bytes.len() {
            return Err(format!("line {line}: unterminated string in `{key}`"));
        }
        values.push(inner[start..at].to_string());
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        if bytes[at] != b',' {
            return Err(format!(
                "line {line}: expected a comma between strings in `{key}`"
            ));
        }
        at += 1;
    }
    Ok(values)
}

fn unhex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("stored intent contains odd-length hex".to_string());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char)
                .to_digit(16)
                .ok_or_else(|| "stored intent contains invalid hex".to_string())?;
            let low = (pair[1] as char)
                .to_digit(16)
                .ok_or_else(|| "stored intent contains invalid hex".to_string())?;
            Ok(((high << 4) | low) as u8)
        })
        .collect()
}

fn decode_text(value: &str) -> Result<String> {
    String::from_utf8(unhex(value)?)
        .map_err(|_| "stored intent contains text that is not UTF-8".to_string())
}

fn decode_list(value: &str) -> Result<Vec<String>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(decode_text).collect()
}

fn split_legacy_list(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        vec![value.to_string()]
    }
}

/// Read the ledger, folding a pre-ledger `.jails/app-state-v1` in first.
///
/// The old file is removed once folded. Leaving it would be worse than never
/// having read it: two registries drift, and the stale one is the one that
/// still answers "already applied" for an intent the manifest has since
/// changed.
fn read_state(root: &Path) -> Result<AppState> {
    let mut ledger = crate::ledger::load(root)?;
    if migrate_app_state(root, &mut ledger)? {
        crate::ledger::save(root, &ledger)?;
    }
    Ok(AppState { ledger })
}

/// Fold `.jails/app-state-v1` into the ledger. Returns whether anything moved.
fn migrate_app_state(root: &Path, into: &mut crate::ledger::Ledger) -> Result<bool> {
    let path = root.join(STATE_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };

    let mut lines = text.lines();
    let mut intents = Vec::new();
    match lines.next() {
        Some("schema=1") => {
            for line in lines.filter(|line| !line.trim().is_empty()) {
                intents.push(ResolvedIntent::from_legacy(root, line)?);
            }
        }
        Some("schema=2") => {
            for line in lines.filter(|line| !line.trim().is_empty()) {
                if let Some(encoded) = line.strip_prefix("intent=") {
                    intents.push(
                        ResolvedIntent::decode(encoded)
                            .map_err(|error| format!("{}: {error}", path.display()))?,
                    );
                } else if let Some(encoded) = line.strip_prefix("legacy=") {
                    let decoded = decode_text(encoded)
                        .map_err(|error| format!("{}: {error}", path.display()))?;
                    intents.push(ResolvedIntent::from_legacy(root, &decoded)?);
                } else {
                    return Err(format!("{} has an invalid state entry", path.display()));
                }
            }
        }
        _ => {
            return Err(format!(
                "{} has an unsupported or missing schema header",
                path.display()
            ));
        }
    }

    for intent in &intents {
        intent.record_onto(crate::ledger::entry_mut(
            into,
            &intent.recipe(),
            &intent.name,
            intent.package.as_deref(),
        ));
    }
    fs::remove_file(&path)
        .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `on`/`yields` are the names; the `strategy_*` spellings still parse.
    ///
    /// The alias is not politeness -- `.jails/app.toml` is a file people have
    /// already written, and four manifests in `examples/` use the old keys.
    /// What the alias must not do is let one intent set the same reference
    /// twice under two names, which would make the state key depend on which
    /// line the parser saw last.
    #[test]
    fn on_and_yields_are_the_names_and_the_strategy_spellings_still_parse() {
        let canonical = r#"
                schema = 1

                [[generate]]
                kind = "usecase"
                name = "QueueWork"
                on = "WorkItem"
                yields = "WorkQueued"
        "#;
        let legacy = canonical
            .replace("on = ", "strategy_on = ")
            .replace("yields = ", "strategy_yields = ");

        let (_, new_intents) = parse_manifest(canonical).unwrap();
        let (_, old_intents) = parse_manifest(&legacy).unwrap();
        assert_eq!(new_intents[0].strategy_on.as_deref(), Some("WorkItem"));
        assert_eq!(
            new_intents[0].strategy_yields.as_deref(),
            Some("WorkQueued")
        );
        // Identical intents, so identical state keys: renaming the key in a
        // manifest must not make `app apply` see a new intent and refuse on
        // files that already exist.
        assert_eq!(new_intents[0].key(), old_intents[0].key());
    }

    #[test]
    fn one_reference_may_not_be_set_under_both_spellings() {
        let error = parse_manifest(
            r#"
                schema = 1

                [[generate]]
                kind = "usecase"
                name = "QueueWork"
                on = "WorkItem"
                strategy_on = "SomethingElse"
        "#,
        )
        .unwrap_err();
        assert!(error.contains("deprecated alias"), "{error}");
    }

    #[test]
    fn parses_a_domain_blind_application_manifest() {
        let (_, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["db", "api"]

                [[generate]]
                kind = "enum"
                name = "Status"
                fields = ["PENDING", "DONE"]

                [[generate]]
                kind = "scaffold"
                name = "Task"
                fields = ["id:uuid@pk", "status:Status"]
                indexes = ["status, id"]
            "#,
        )
        .unwrap();
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[1].name, "Task");
        assert_eq!(intents[1].indexes, ["status, id"]);
    }

    #[test]
    fn rejects_unknown_keys_instead_of_silently_ignoring_them() {
        let error = parse_manifest(
            r#"
                schema = 1
                capabilities = []
                [[generate]]
                kind = "record"
                name = "Task"
                feilds = ["id:uuid"]
            "#,
        )
        .unwrap_err();
        assert!(error.contains("feilds"), "{error}");
    }

    #[test]
    fn a_capability_only_application_is_valid() {
        let (manifest, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["api", "actuator"]
            "#,
        )
        .unwrap();
        assert_eq!(manifest.capabilities.len(), 2);
        assert!(intents.is_empty());
    }

    /// The ledger is the storage now, so the round trip has to go through it --
    /// including the TOML escaping of a name with an accent and a field with a
    /// comma inside a type argument, which is exactly what the hand-rolled hex
    /// encoding existed to survive.
    #[test]
    fn stored_intents_round_trip_commas_unicode_and_typed_references() {
        let intent = ResolvedIntent {
            kind: ArtifactKind::Record,
            name: "Résumé".to_string(),
            fields: vec![
                "totals:map<string,double>".to_string(),
                "id:uuid@pk".to_string(),
            ],
            timestamps: true,
            indexes: vec!["status, id".to_string(), "created_at".to_string()],
            package: Some("accounting.model".to_string()),
            strategy_on: Some("Input".to_string()),
            strategy_yields: Some("Output".to_string()),
        };

        let root = std::env::temp_dir().join(format!("jails-app-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut state = read_state(&root).unwrap();
        state.record(&root, &intent).unwrap();

        let reread = read_state(&root).unwrap();
        assert!(reread.is_applied(&intent), "round trip");
        assert_eq!(
            ResolvedIntent::from_applied(reread.entry(&intent).unwrap()).unwrap(),
            intent
        );
    }

    /// A project written by an older jails keeps its history.
    #[test]
    fn a_pre_ledger_app_state_is_folded_in_rather_than_ignored() {
        let root = std::env::temp_dir().join(format!("jails-app-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".jails")).unwrap();
        // schema=1: kind|name|package|fields|timestamps|indexes|on|yields
        fs::write(
            root.join(STATE_FILE),
            // The old key joined arrays with a comma, which is ambiguous for
            // `map<K,V>` -- so a legacy field list is folded in whole rather
            // than guessed at. One field is the case that is unambiguous.
            "schema=1\nrecord|Note|domain|title:string!|false|||\n",
        )
        .unwrap();

        let state = read_state(&root).unwrap();
        let intent = ResolvedIntent {
            kind: ArtifactKind::Record,
            name: "Note".to_string(),
            fields: vec!["title:string!".to_string()],
            timestamps: false,
            indexes: Vec::new(),
            package: Some("domain".to_string()),
            strategy_on: None,
            strategy_yields: None,
        };
        assert!(state.is_applied(&intent), "the old entry still counts");
        assert!(
            !root.join(STATE_FILE).exists(),
            "and the second registry is gone, not left to drift"
        );
    }
}
