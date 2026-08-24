//! Reading `.jails/app.toml`, and refusing what it does not recognise.
//!
//! A **closed** schema, the same rule as `jails.toml` and `ledger.toml` and for
//! the same reason: `apply` acts on this file, so a key it silently ignored
//! would be an intent somebody believed they had declared. `strategy_on` and
//! `strategy_yields` still parse as deprecated aliases of `on` and `yields`,
//! because they shipped in a user-facing file format -- and naming one
//! reference under both spellings is an error rather than a last-one-wins.
//!
//! Hand-parsed, because jails has two dependencies and intends to keep it that
//! way.

use super::*;

pub(super) fn manifest_path(root: &Path, requested: Option<&Path>) -> Result<PathBuf> {
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

pub(super) fn read_manifest(path: &Path) -> Result<(Manifest, Vec<ResolvedIntent>)> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    parse_manifest(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub(super) fn parse_manifest(text: &str) -> Result<(Manifest, Vec<ResolvedIntent>)> {
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
    // On identity, not on identity-plus-content. Keyed on both, a manifest
    // declaring one entity twice with *different* fields was accepted and both
    // entries applied -- the second silently overwriting the first's row.
    // R1.2's gate: duplicate identity refuses before any write.
    let mut seen = HashSet::new();
    for intent in &resolved {
        let recipe = intent.recipe();
        // The *recorded* name, so `fetcher Acquirer` and `fetcher
        // AcquirerFetcher` are caught as the one entity they generate into
        // rather than accepted as two and applied over each other.
        let name = intent.recorded_name();
        let key = intent.key(&recipe, &name);
        if !seen.insert((
            key.recipe.to_string(),
            key.name.to_string(),
            key.package.to_string(),
        )) {
            return Err(format!(
                "`{key}` is declared twice.\n       fix: one entity has one declaration; give \
                 the second a different name or package, or merge the two."
            ));
        }
    }
    Ok((manifest, resolved))
}

pub(super) fn strip_comment(line: &str) -> &str {
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

pub(super) fn string<'a>(value: &'a str, line: usize, key: &str) -> Result<&'a str> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("line {line}: `{key}` must be a double-quoted string"))
}

pub(super) fn string_array(value: &str, line: usize, key: &str) -> Result<Vec<String>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One entity, two declarations, different fields.
    ///
    /// The duplicate check keyed on identity *plus content*, so this was
    /// accepted and both entries applied — the second silently overwriting the
    /// first's recorded row. Verified against the previous binary, which planned
    /// `pending generate record Note a:string` and
    /// `pending generate record Note b:int` from this exact manifest.
    #[test]
    fn one_entity_declared_twice_with_different_content_refuses() {
        let error = parse_manifest(
            "schema = 1\n\n\
             [[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"a:string\"]\n\n\
             [[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"b:int\"]\n",
        )
        .unwrap_err();
        assert!(error.contains("declared twice"), "{error}");
        assert!(
            error.contains("record Note"),
            "the message names it: {error}"
        );
    }

    /// The same name in a different package is a different entity, and must
    /// still be allowed.
    #[test]
    fn the_same_name_in_another_package_is_not_a_duplicate() {
        assert!(
            parse_manifest(
                "schema = 1\n\n\
                 [[generate]]\nkind = \"record\"\nname = \"Note\"\npackage = \"a\"\n\n\
                 [[generate]]\nkind = \"record\"\nname = \"Note\"\npackage = \"b\"\n",
            )
            .is_ok()
        );
    }

    /// And a different recipe for the same name is a different entity too.
    #[test]
    fn a_different_recipe_for_the_same_name_is_not_a_duplicate() {
        assert!(
            parse_manifest(
                "schema = 1\n\n\
                 [[generate]]\nkind = \"record\"\nname = \"Note\"\n\n\
                 [[generate]]\nkind = \"value\"\nname = \"Note\"\n",
            )
            .is_ok()
        );
    }
}
