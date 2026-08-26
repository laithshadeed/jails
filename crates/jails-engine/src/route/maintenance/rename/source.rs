//! Java source-name validation and destination discovery.

use super::*;

pub(super) fn validate(old: &str, new: &str) -> Result<()> {
    for (label, name) in [("old", old), ("new", new)] {
        if name.is_empty() {
            return Err(format!(
                "the {label} name is empty.\n       fix: pass one simple Java type name."
            )
            .into());
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter.\n       \
                 fix: pass one simple Java type name."
            )
            .into());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a \
                 package path.\n       fix: pass the simple name (`Reward`, not `com.example.Reward`)."
            )
            .into());
        }
    }
    if old == new {
        return Err(
            "the old and new names are the same.\n       fix: choose a distinct target name."
                .into(),
        );
    }
    Ok(())
}

fn destination_of(source: &ProjectPath, old: &str, new: &str) -> Result<ProjectPath> {
    let renamed =
        jails_java::identifier::renamed_path(std::path::Path::new(&source.to_string()), old, new);
    ProjectPath::parse(
        renamed
            .to_str()
            .ok_or_else(|| format!("`{}` is not valid UTF-8", renamed.display()))?,
    )
}

pub(super) fn rename_destination(
    store: &ObservedStore,
    source: &ProjectPath,
    old: &str,
    new: &str,
    resource: Option<&EntityId>,
) -> Result<ProjectPath> {
    let Some(entity) = resource else {
        return destination_of(source, old, new);
    };
    if !owned_by(store, source, entity) {
        return Ok(source.clone());
    }
    let path = std::path::Path::new(source.as_str());
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(source.clone());
    };
    let Some(position) = stem.find(old) else {
        return Ok(source.clone());
    };
    let mut renamed = stem.to_string();
    renamed.replace_range(position..position + old.len(), new);
    let destination = path.with_file_name(format!("{renamed}.java"));
    ProjectPath::parse(
        destination
            .to_str()
            .ok_or_else(|| format!("`{}` is not valid UTF-8", destination.display()))?,
    )
}

pub(super) fn walked_directories(sources: &[ProjectPath]) -> BTreeSet<ProjectPath> {
    let mut out = BTreeSet::new();
    for source in sources {
        let text = source.to_string();
        if let Some((directory, _)) = text.rsplit_once('/')
            && let Ok(path) = ProjectPath::parse(directory)
        {
            out.insert(path);
        }
    }
    out
}
