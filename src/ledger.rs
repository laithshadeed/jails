//! `.jails/ledger.toml`: what jails has done to this project.
//!
//! ## The seven-way split this replaces
//!
//! `abstract.md` §4.5 counted the places jails recorded its own work:
//! `jails.toml`'s capability list, `.jails/app.toml`'s wanted capabilities and
//! intents, `.jails/app-state-v1`, `.jails/intents/*.files`, `.jails/files` and
//! `.jails/models/*`. Two of those were intent registries **keyed differently**
//! — `app-state-v1` on the full argument set, `intents/` on kind+name+package —
//! and that difference was not a nearby cause of the §9.7 bug, it *was* the bug.
//!
//! One rule replaces it: **identity is `(recipe, name, package)`; arguments are
//! content.** Evans's entity-versus-value-object distinction and nothing more
//! exotic. An intent whose `fields` line changed is the *same* entity with new
//! content, which is precisely the input the regenerate-and-merge repair needs
//! — it has the old content to hand because the entity remembered it.
//!
//! ## What stays out, and why
//!
//! `jails.toml` is **not** folded in, against `abstract.md` §6.3's first
//! instinct and in line with the question §8.2 leaves open. It is a file people
//! edit: `[layout]` is theirs, and `CLAUDE.md` protects it with byte-preserving
//! splices for that reason. Merging hand-owned configuration into machine-owned
//! state trades that property away and buys only a smaller file count.
//! `.jails/app.toml` stays out for the same reason.
//!
//! So the boundary is: **`.jails/ledger.toml` is jails' bookkeeping and is
//! never hand-edited; everything else under `.jails/` is yours.** That is a
//! real boundary, unlike a split across five machine-owned files.
//!
//! ## Why it is still diffable
//!
//! `plan.md` §11.2 argues from `openapi-generator`'s `FILES` that a sorted,
//! separator-normalised, one-path-per-line list earns its place *because* it is
//! diffable and not derived. That argument survives: paths stay sorted, stay
//! `/`-normalised, and appear one per line inside their array. A reviewer reads
//! the same diff they read before.

use jails_support::Result;
use jails_support::apply;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const LEDGER: &str = ".jails/ledger.toml";

/// Everything jails has applied to one project.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ledger {
    /// The jails that last wrote this ledger.
    pub(crate) version: String,
    pub(crate) applied: Vec<Applied>,
    pub(crate) models: Vec<Model>,
}

impl Ledger {
    /// The only empty ledger anyone may construct: the one that stands for a
    /// project jails has never written to.
    ///
    /// `load` reaches this on `NotFound` and on nothing else. Every other read
    /// failure is an error, because an empty ledger is a *claim* -- that jails
    /// owns nothing here -- and a permission error is not evidence for it.
    pub(crate) fn empty() -> Self {
        Ledger {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Ledger::default()
        }
    }
}

/// Whether anyone ever recorded *what* an entity was built from.
///
/// This used to be guessed from content by `has_spec()`, which could not tell a
/// valid zero-argument `app` intent from a row `generate` wrote and never held
/// a spec for: both have empty fields, no indexes and no references. The two
/// need opposite treatment -- the first is a manifest entry to three-way merge
/// against, the second must never be read as a manifest entry that has since
/// been emptied -- so presence is data now, written by whichever writer owns
/// the spec column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecPresence {
    /// An `app` manifest intent. True even when every argument is defaulted.
    Present,
    /// A direct `generate` row: paths, and no spec was ever offered.
    Absent,
    /// A row written before this key existed. **Not** resolvable by looking at
    /// the content: a legacy row whose fields happen to match today's manifest
    /// is still a row of unknown origin. Only a user-requested adoption may
    /// turn it into a named owner.
    UnknownLegacy,
}

/// One intent, recorded once. Identity is `(recipe, name, package)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    pub(crate) recipe: String,
    pub(crate) name: String,
    /// The `--package` override, or empty for the conventional layer.
    pub(crate) package: String,
    /// Whether a spec was recorded, as data rather than as a guess.
    pub(crate) spec: SpecPresence,
    /// Content, not identity: this is what the entity currently says.
    pub(crate) fields: Vec<String>,
    pub(crate) indexes: Vec<String>,
    pub(crate) on: String,
    pub(crate) yields: String,
    pub(crate) timestamps: bool,
    /// The paths this intent wrote, sorted and `/`-normalised.
    ///
    /// Recorded rather than recomputed. `plan.md` §11.2: after a jails upgrade
    /// a recomputed path gives you today's answer for yesterday's file, and
    /// `destroy` would then strand what it claimed to delete.
    pub(crate) files: Vec<String>,
}

impl Applied {
    /// Two records are the same entity when these three agree.
    pub(crate) fn is(&self, recipe: &str, name: &str, package: Option<&str>) -> bool {
        self.recipe == recipe && self.name == name && self.package == package.unwrap_or_default()
    }

    /// Mark this row as one `app apply` owns the spec of.
    ///
    /// Unconditional: a manifest intent with no arguments at all is still a
    /// manifest intent, and that is exactly the case content could not express.
    pub(crate) fn claim_spec(&mut self) {
        self.spec = SpecPresence::Present;
    }
}

/// A field spec recorded so a later generator can build from the model rather
/// than making the reader retype it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Model {
    pub(crate) name: String,
    pub(crate) package: String,
    pub(crate) fields: Vec<String>,
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Read the ledger, failing closed.
///
/// The `let Ok(..) else` this replaces turned **every** read failure into an
/// empty ledger: a permission error, a non-UTF-8 file, an I/O error mid-read.
/// Empty is not a neutral value here -- it is the claim that jails owns nothing
/// in this project -- so `destroy` would report there is nothing to delete over
/// files that are right there, and a write would overwrite the only record of
/// what jails owns. Only `NotFound` is evidence for that claim.
pub(crate) fn load(root: &Path) -> Result<Ledger> {
    let path = root.join(LEDGER);
    match fs::read_to_string(&path) {
        Ok(source) => parse(&source).map_err(|error| format!("{}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Ledger::empty()),
        Err(error) => Err(contextual_read_error(&path, error)),
    }
}

/// Name the failure and what to do about it, rather than the raw `io::Error`.
///
/// `doctor`'s rule: a report a reader cannot act on costs more than no report.
fn contextual_read_error(path: &Path, error: io::Error) -> String {
    let fix = match error.kind() {
        io::ErrorKind::PermissionDenied => {
            "\n       fix: restore read access to this file; jails refuses to \
             treat an unreadable ledger as an empty one."
        }
        io::ErrorKind::InvalidData => {
            "\n       fix: this file is not valid UTF-8. Restore it from version \
             control; jails will not guess what it recorded."
        }
        _ => {
            "\n       fix: restore this file from version control, or remove it \
             only if you accept that jails then owns nothing in this project."
        }
    };
    format!("failed to read {}: {error}{fix}", path.display())
}

/// A closed schema, like `jails.toml` and `.jails/app.toml`.
///
/// An unknown key is an error rather than silence, for the reason those two
/// give: a ledger that quietly ignored a key it did not understand would report
/// a project as unmodified when it is not, and `destroy` acts on that report.
fn parse(source: &str) -> std::result::Result<Ledger, String> {
    let mut ledger = Ledger::default();
    let mut applied: Option<Applied> = None;
    let mut model: Option<Model> = None;

    for (number, raw) in logical_lines(source) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[applied]]" {
            flush(&mut ledger, &mut applied, &mut model);
            applied = Some(Applied {
                recipe: String::new(),
                name: String::new(),
                package: String::new(),
                // A row that never says. `has_spec` below is the only thing
                // that resolves it, and content must not.
                spec: SpecPresence::UnknownLegacy,
                fields: Vec::new(),
                indexes: Vec::new(),
                on: String::new(),
                yields: String::new(),
                timestamps: false,
                files: Vec::new(),
            });
            continue;
        }
        if line == "[[model]]" {
            flush(&mut ledger, &mut applied, &mut model);
            model = Some(Model {
                name: String::new(),
                package: String::new(),
                fields: Vec::new(),
            });
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {number}: expected `key = value`, found `{line}`"))?;
        let (key, value) = (key.trim(), value.trim());

        if let Some(entry) = applied.as_mut() {
            match key {
                "recipe" => entry.recipe = string(value, number)?,
                "name" => entry.name = string(value, number)?,
                "package" => entry.package = string(value, number)?,
                "on" => entry.on = string(value, number)?,
                "yields" => entry.yields = string(value, number)?,
                "timestamps" => entry.timestamps = boolean(value, number)?,
                "has_spec" => {
                    entry.spec = if boolean(value, number)? {
                        SpecPresence::Present
                    } else {
                        SpecPresence::Absent
                    }
                }
                "fields" => entry.fields = array(value, number)?,
                "indexes" => entry.indexes = array(value, number)?,
                "files" => entry.files = array(value, number)?,
                _ => return Err(format!("line {number}: unknown [[applied]] key `{key}`")),
            }
        } else if let Some(entry) = model.as_mut() {
            match key {
                "name" => entry.name = string(value, number)?,
                "package" => entry.package = string(value, number)?,
                "fields" => entry.fields = array(value, number)?,
                _ => return Err(format!("line {number}: unknown [[model]] key `{key}`")),
            }
        } else {
            match key {
                "version" => ledger.version = string(value, number)?,
                _ => return Err(format!("line {number}: unknown key `{key}`")),
            }
        }
    }
    flush(&mut ledger, &mut applied, &mut model);
    if ledger.version.is_empty() {
        // An empty or version-less file is not "a project with no history"; it
        // is a ledger that was truncated, half-written or hand-made. `NotFound`
        // is the one shape that means no history, and it never reaches here.
        return Err(
            "missing top-level `version`; an empty or truncated ledger is an error, \
             not an empty project"
                .to_string(),
        );
    }
    Ok(ledger)
}

/// Join an array that the renderer wrapped across lines back into one logical
/// line, keeping the number of the line it started on for error messages.
///
/// Paths are written one per line precisely so a diff names the path that
/// moved (`plan.md` §11.2), which means the reader has to put them back
/// together. Doing that here keeps the rest of the parser line-oriented.
fn logical_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        match pending.as_mut() {
            Some((_, buffer)) => {
                buffer.push_str(raw.trim());
                if raw.trim_end().ends_with(']') {
                    let (start, joined) = pending.take().expect("checked above");
                    out.push((start, joined));
                }
            }
            None => {
                let trimmed = raw.trim();
                let unterminated = trimmed.contains("= [") && !trimmed.ends_with(']');
                if unterminated {
                    pending = Some((number, trimmed.to_string()));
                } else {
                    out.push((number, trimmed.to_string()));
                }
            }
        }
    }
    if let Some(unfinished) = pending {
        out.push(unfinished);
    }
    out
}

fn flush(ledger: &mut Ledger, applied: &mut Option<Applied>, model: &mut Option<Model>) {
    if let Some(entry) = applied.take() {
        ledger.applied.push(entry);
    }
    if let Some(entry) = model.take() {
        ledger.models.push(entry);
    }
}

fn string(value: &str, line: usize) -> std::result::Result<String, String> {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .map(|inner| inner.replace("\\\"", "\"").replace("\\\\", "\\"))
        .ok_or_else(|| format!("line {line}: expected a quoted string, found `{value}`"))
}

fn boolean(value: &str, line: usize) -> std::result::Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!(
            "line {line}: expected true or false, found `{value}`"
        )),
    }
}

fn array(value: &str, line: usize) -> std::result::Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| format!("line {line}: expected an array, found `{value}`"))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    elements(inner)
        .into_iter()
        .filter(|item| !item.trim().is_empty())
        .map(|item| string(item.trim(), line))
        .collect()
}

/// Split an array body on its *separating* commas.
///
/// A plain `split(',')` cuts `"totals:map<string,double>"` in half, and jails
/// documents that type -- so the field spec that most needs recording was the
/// one the storage could not hold. Quotes are tracked, and a `\"` inside a
/// string does not end it.
fn elements(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => {
                current.push(character);
                escaped = true;
            }
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            ',' if !quoted => items.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    items.push(current);
    items
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

pub(crate) fn save(root: &Path, ledger: &Ledger) -> Result<()> {
    apply::atomically(root.join(LEDGER), render(ledger))
}

fn render(ledger: &Ledger) -> String {
    let mut out = String::new();
    out.push_str(
        "# jails' own bookkeeping. Written by jails, never hand-edited -- `jails.toml`\n\
         # and `.jails/app.toml` are the files that are yours.\n\
         #\n\
         # Identity is (recipe, name, package). Everything else is content, which is\n\
         # what makes an edited intent an update to a known entity rather than a new\n\
         # one arriving against files that already exist.\n",
    );
    out.push_str(&format!("version = {}\n", quoted(&ledger.version)));

    let mut applied = ledger.applied.clone();
    applied.sort_by(|a, b| (&a.recipe, &a.name, &a.package).cmp(&(&b.recipe, &b.name, &b.package)));
    for entry in &applied {
        out.push_str("\n[[applied]]\n");
        out.push_str(&format!("recipe = {}\n", quoted(&entry.recipe)));
        out.push_str(&format!("name = {}\n", quoted(&entry.name)));
        out.push_str(&format!("package = {}\n", quoted(&entry.package)));
        if !entry.fields.is_empty() {
            out.push_str(&format!("fields = {}\n", quoted_array(&entry.fields)));
        }
        if !entry.indexes.is_empty() {
            out.push_str(&format!("indexes = {}\n", quoted_array(&entry.indexes)));
        }
        if !entry.on.is_empty() {
            out.push_str(&format!("on = {}\n", quoted(&entry.on)));
        }
        if !entry.yields.is_empty() {
            out.push_str(&format!("yields = {}\n", quoted(&entry.yields)));
        }
        if entry.timestamps {
            out.push_str("timestamps = true\n");
        }
        // Omitted for a legacy row, so re-reading it keeps saying "unknown"
        // rather than inventing an answer this binary never learned.
        match entry.spec {
            SpecPresence::Present => out.push_str("has_spec = true\n"),
            SpecPresence::Absent => out.push_str("has_spec = false\n"),
            SpecPresence::UnknownLegacy => {}
        }
        if !entry.files.is_empty() {
            out.push_str(&format!("files = {}\n", quoted_array(&entry.files)));
        }
    }

    let mut models = ledger.models.clone();
    models.sort_by(|a, b| (&a.name, &a.package).cmp(&(&b.name, &b.package)));
    for entry in &models {
        out.push_str("\n[[model]]\n");
        out.push_str(&format!("name = {}\n", quoted(&entry.name)));
        out.push_str(&format!("package = {}\n", quoted(&entry.package)));
        if !entry.fields.is_empty() {
            out.push_str(&format!("fields = {}\n", quoted_array(&entry.fields)));
        }
    }
    out
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// One item per line, so a diff shows the path that moved rather than the whole
/// array. This is `plan.md` §11.2's property, kept.
fn quoted_array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let items: Vec<String> = values.iter().map(|value| quoted(value)).collect();
    format!("[\n  {},\n]", items.join(",\n  "))
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// A project-relative, `/`-separated path, refusing anything that escapes.
pub(crate) fn relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "generated path {} escapes project root {}",
            path.display(),
            root.display()
        )
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "generated path {} is not a confined relative path",
                    path.display()
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err("refusing to record the project root as a generated file".to_string());
    }
    Ok(parts.join("/"))
}

pub(crate) fn absolute(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "invalid generated path `{relative}` in {LEDGER}; expected a confined relative path"
        ));
    }
    Ok(root.join(path))
}

/// The record for one entity, created blank if this is the first sighting.
///
/// Two writers reach the same row -- `generate` recording the paths it wrote
/// and `app apply` recording the spec it was built from -- and neither owns the
/// other's columns. Replacing the row wholesale is how one of them silently
/// erases the other, so both come through here and set only their own fields.
pub(crate) fn entry_mut<'a>(
    ledger: &'a mut Ledger,
    recipe: &str,
    name: &str,
    package: Option<&str>,
) -> &'a mut Applied {
    let position = ledger
        .applied
        .iter()
        .position(|entry| entry.is(recipe, name, package));
    match position {
        Some(index) => &mut ledger.applied[index],
        None => {
            ledger.applied.push(Applied {
                recipe: recipe.to_string(),
                name: name.to_string(),
                package: package.unwrap_or_default().to_string(),
                // A row jails is creating now genuinely has no spec. `app
                // apply` calls `claim_spec` when it is the one asking; a row
                // `generate` created keeps this and is never mistaken for an
                // emptied manifest entry.
                spec: SpecPresence::Absent,
                fields: Vec::new(),
                indexes: Vec::new(),
                on: String::new(),
                yields: String::new(),
                timestamps: false,
                files: Vec::new(),
            });
            ledger.applied.last_mut().expect("just pushed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Ledger {
        Ledger {
            version: "0.1.0".to_string(),
            applied: vec![Applied {
                recipe: "scaffold".to_string(),
                name: "Note".to_string(),
                package: String::new(),
                spec: SpecPresence::Present,
                fields: vec!["id:uuid@pk".to_string(), "title:string!".to_string()],
                indexes: vec!["title".to_string()],
                on: String::new(),
                yields: String::new(),
                timestamps: true,
                files: vec![
                    "src/main/java/com/example/demo/domain/Note.java".to_string(),
                    "src/test/java/com/example/demo/domain/NoteTest.java".to_string(),
                ],
            }],
            models: vec![Model {
                name: "Note".to_string(),
                package: String::new(),
                fields: vec!["id:uuid@pk".to_string()],
            }],
        }
    }

    #[test]
    fn a_ledger_round_trips_through_its_own_format() {
        let rendered = render(&sample());
        assert_eq!(parse(&rendered).unwrap(), sample(), "{rendered}");
    }

    #[test]
    fn an_unknown_key_is_an_error_rather_than_silence() {
        // The same closed-set rule `jails.toml` uses, for a sharper reason:
        // `destroy` acts on this file, so a key jails silently ignored would
        // make it delete the wrong set.
        let error = parse("version = \"1\"\n\n[[applied]]\nrecipie = \"scaffold\"\n").unwrap_err();
        assert!(
            error.contains("unknown [[applied]] key `recipie`"),
            "{error}"
        );
    }

    #[test]
    fn identity_is_recipe_name_and_package_only() {
        let first = sample().applied.remove(0);
        let mut edited = first.clone();
        edited.fields = vec!["id:uuid@pk".to_string()];

        assert!(edited.is("scaffold", "Note", None), "still the same entity");
        assert_ne!(
            first.fields, edited.fields,
            "but its content changed, which is what a merge needs to know"
        );
    }

    /// The case content could not express, and the reason `has_spec()` had to
    /// become data: both of these rows have no fields, no indexes and no
    /// references, and they mean opposite things.
    #[test]
    fn a_zero_argument_app_intent_is_not_a_row_generate_wrote() {
        let bare = Applied {
            recipe: "record".to_string(),
            name: "Marker".to_string(),
            package: String::new(),
            spec: SpecPresence::Absent,
            fields: Vec::new(),
            indexes: Vec::new(),
            on: String::new(),
            yields: String::new(),
            timestamps: false,
            files: vec!["src/main/java/com/example/demo/domain/Marker.java".to_string()],
        };
        let mut claimed = bare.clone();
        claimed.claim_spec();

        assert_ne!(
            bare.spec, claimed.spec,
            "identical content, opposite origin"
        );
        assert_eq!(claimed.spec, SpecPresence::Present);

        let ledger = Ledger {
            version: "0.1.0".to_string(),
            applied: vec![claimed.clone()],
            models: Vec::new(),
        };
        let rendered = render(&ledger);
        assert!(rendered.contains("has_spec = true"), "{rendered}");
        assert_eq!(
            parse(&rendered).unwrap().applied[0].spec,
            SpecPresence::Present,
            "a spec with every argument defaulted survives the round trip"
        );

        let paths_only = render(&Ledger {
            applied: vec![bare],
            ..ledger
        });
        assert!(paths_only.contains("has_spec = false"), "{paths_only}");
    }

    /// A row written before the key existed stays unknown. Its content may
    /// happen to match a manifest entry exactly; that is not evidence, and
    /// resolving it is a user-requested adoption, not a parse.
    #[test]
    fn a_legacy_row_without_the_key_stays_unknown_through_a_round_trip() {
        let parsed = parse(
            "version = \"0.1.0\"\n\n[[applied]]\nrecipe = \"record\"\nname = \"Note\"\n\
             fields = [\"title:string!\"]\n",
        )
        .unwrap();
        assert_eq!(parsed.applied[0].spec, SpecPresence::UnknownLegacy);

        let rendered = render(&parsed);
        assert!(
            !rendered.contains("has_spec"),
            "re-rendering must not invent an answer this binary never learned: {rendered}"
        );
        assert_eq!(
            parse(&rendered).unwrap().applied[0].spec,
            SpecPresence::UnknownLegacy
        );
    }

    /// Only `NotFound` may construct empty state. Every other read failure is
    /// the claim "jails owns nothing here" made without evidence for it.
    #[test]
    fn an_absent_ledger_is_empty_and_an_unreadable_one_is_an_error() {
        let root = scratch("fail-closed");
        assert_eq!(load(&root).unwrap(), Ledger::empty(), "no file yet");

        fs::create_dir_all(root.join(".jails")).unwrap();
        let path = root.join(LEDGER);

        fs::write(&path, "").unwrap();
        let error = load(&root).unwrap_err();
        assert!(error.contains("missing top-level `version`"), "{error}");

        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let error = load(&root).unwrap_err();
        assert!(error.contains("failed to read"), "{error}");
        assert!(error.contains("fix:"), "{error}");

        fs::write(&path, "version = \"0.1.0\"\nschema = 2\n").unwrap();
        let error = load(&root).unwrap_err();
        assert!(
            error.contains("unknown key `schema`"),
            "a ledger from a newer jails is refused, not half-read: {error}"
        );

        fs::remove_dir_all(&root).unwrap();
    }

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "jails-ledger-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// `map<string,double>` is a documented field type, and a naive
    /// `split(',')` cuts it in half -- silently, into two specs that both parse.
    #[test]
    fn a_comma_inside_a_type_argument_does_not_end_the_element() {
        let parsed = parse(
            "version = \"0.1.0\"\n\n[[applied]]\nrecipe = \"record\"\nname = \"Order\"\n\
             fields = [\"totals:map<string,double>\", \"id:uuid@pk\"]\n",
        )
        .unwrap();
        assert_eq!(
            parsed.applied[0].fields,
            vec!["totals:map<string,double>", "id:uuid@pk"]
        );
    }

    #[test]
    fn paths_are_recorded_one_per_line_so_a_diff_shows_which_one_moved() {
        let rendered = render(&sample());
        assert!(
            rendered
                .contains("files = [\n  \"src/main/java/com/example/demo/domain/Note.java\",\n"),
            "{rendered}"
        );
    }

    #[test]
    fn entries_are_sorted_so_two_machines_write_the_same_bytes() {
        let mut ledger = sample();
        ledger.applied.push(Applied {
            recipe: "record".to_string(),
            name: "Alpha".to_string(),
            ..ledger.applied[0].clone()
        });
        let rendered = render(&ledger);
        assert!(
            rendered.find("\"record\"").unwrap() < rendered.find("\"scaffold\"").unwrap(),
            "recipe orders first, and `record` sorts before `scaffold`: {rendered}"
        );
    }

    #[test]
    fn a_path_that_escapes_the_project_is_refused() {
        let root = Path::new("/tmp/project");
        assert!(relative(root, Path::new("/etc/passwd")).is_err());
        assert!(absolute(root, "../outside").is_err());
        assert_eq!(
            relative(root, &root.join("src/A.java")).unwrap(),
            "src/A.java"
        );
    }
}
