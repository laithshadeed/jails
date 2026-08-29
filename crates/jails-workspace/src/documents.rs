//! Bounded, lossless edits to reader-owned build documents.
//!
//! These adapters do not try to understand Maven or Gradle as languages. They
//! insert one explicitly owned source-root block, preserve every other byte,
//! and refuse a damaged/edited owned block instead of guessing.

mod build_feature;
mod source_root;

pub(crate) use build_feature::{reconcile_gradle_build_features, reconcile_maven_build_features};
pub(crate) use source_root::{ensure_gradle_source_root, ensure_maven_source_roots};

const DEPENDENCY_MARKER: &str = "jails:dependencies";

pub(crate) fn reconcile_properties(
    text: &str,
    previous: &[jails_contracts::PropertyEntry],
    desired: &[jails_contracts::PropertyEntry],
) -> Result<String, String> {
    use std::collections::{BTreeMap, BTreeSet};

    let previous = previous
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<BTreeMap<_, _>>();
    let desired = desired
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<BTreeMap<_, _>>();
    let owned = previous
        .keys()
        .chain(desired.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut seen = BTreeSet::new();
    let mut output = String::with_capacity(text.len() + desired.len() * 48);

    for line in text.split_inclusive('\n') {
        let Some(key) = property_key(line) else {
            output.push_str(line);
            continue;
        };
        if !owned.contains(key) {
            output.push_str(line);
            continue;
        }
        if !seen.insert(key.to_string()) {
            return Err(format!(
                "properties key `{key}` occurs more than once\n       fix: keep one declaration for the key, then re-plan"
            ));
        }
        if !previous.contains_key(key) {
            return Err(format!(
                "reader-owned properties already declare `{key}`\n       fix: remove the reader-owned key or do not declare it in the canonical model"
            ));
        }
        if has_continuation(line) {
            return Err(format!(
                "managed properties key `{key}` uses a continuation line\n       fix: rewrite the value on one line, then re-plan"
            ));
        }
        if let Some(value) = desired.get(key) {
            output.push_str(key);
            output.push('=');
            output.push_str(&escape_property_value(value));
            output.push_str(newline);
        }
    }

    let missing = desired
        .iter()
        .filter(|(key, _)| !seen.contains(**key))
        .collect::<Vec<_>>();
    if !missing.is_empty() && !output.is_empty() && !output.ends_with('\n') {
        output.push_str(newline);
    }
    for (key, value) in missing {
        output.push_str(key);
        output.push('=');
        output.push_str(&escape_property_value(value));
        output.push_str(newline);
    }
    Ok(output)
}

const COMPOSE_HEADER: &str = "# Local development services. `jails add` / `jails remove` own the marked\n# blocks; `jails run` starts everything here.\n";

/// Reconcile one marked Compose service without claiming the surrounding YAML.
///
/// `previous` is the exact compiler block accepted in the lock, `text` is the
/// live reader document, and `desired` is the next compiler block. The block
/// itself therefore has normal BASE/OURS/THEIRS behavior while every byte
/// outside it is copied verbatim.
pub(crate) fn reconcile_compose_service(
    path: &jails_contracts::ProjectPath,
    text: &str,
    service: &str,
    marker: &str,
    previous: Option<&[u8]>,
    desired: Option<&[u8]>,
) -> Result<String, String> {
    let range = compose_marked_range(text, marker)?;
    let current = range.map(|(start, end)| &text.as_bytes()[start..end]);
    if previous.is_none() && range.is_none() && compose_has_service(text, service) {
        return Err(format!(
            "compose service `{service}` already exists outside `{}{marker}`\n       fix: rename the reader-owned service or remove the canonical capability",
            jails_codemod::Marked::OPEN_PREFIX
        ));
    }
    let selected = reconcile_facet_bytes(path, previous, current, desired)?;
    match (range, selected) {
        (Some((start, end)), Some(bytes)) => {
            let replacement = std::str::from_utf8(&bytes)
                .map_err(|_| format!("compiler emitted non-UTF-8 compose facet for `{service}`"))?;
            let mut output = String::with_capacity(text.len() - (end - start) + bytes.len());
            output.push_str(&text[..start]);
            output.push_str(replacement);
            output.push_str(&text[end..]);
            Ok(output)
        }
        (Some((start, end)), None) => {
            let mut output = String::with_capacity(text.len() - (end - start));
            output.push_str(&text[..start]);
            output.push_str(&text[end..]);
            Ok(clean_empty_compose(output))
        }
        (None, Some(bytes)) => {
            let block = std::str::from_utf8(&bytes)
                .map_err(|_| format!("compiler emitted non-UTF-8 compose facet for `{service}`"))?;
            Ok(insert_compose_service(text, block))
        }
        (None, None) => Ok(text.to_string()),
    }
}

fn reconcile_facet_bytes(
    path: &jails_contracts::ProjectPath,
    base: Option<&[u8]>,
    current: Option<&[u8]>,
    desired: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, String> {
    match (base, current, desired) {
        (None, None, Some(desired)) => Ok(Some(desired.to_vec())),
        (None, Some(_), Some(_)) => Err(format!(
            "generated compose facet in `{path}` has no accepted merge base\n       fix: restore `.jails/compiler.lock.json` or move the colliding marked block"
        )),
        (Some(base), Some(current), Some(desired)) if current == base => Ok(Some(desired.to_vec())),
        (Some(base), Some(current), Some(desired)) if desired == base => Ok(Some(current.to_vec())),
        (Some(_), Some(current), Some(desired)) if current == desired => Ok(Some(desired.to_vec())),
        (Some(base), Some(current), Some(desired)) => {
            match crate::merge::three_way(path, base, current, desired)? {
                crate::merge::Merged::Clean(bytes) => Ok(Some(bytes)),
                crate::merge::Merged::Conflicted { hunks } => Err(format!(
                    "`{path}` has {hunks} overlapping compose edit{} between your service and the generator\n       fix: reconcile that marked service by hand; nothing was written",
                    if hunks == 1 { "" } else { "s" }
                )),
            }
        }
        (Some(base), None, Some(desired)) if base == desired => Ok(None),
        (Some(base), None, Some(desired)) => {
            match crate::merge::three_way(path, base, b"", desired)? {
                crate::merge::Merged::Clean(bytes) if bytes.is_empty() => Ok(None),
                crate::merge::Merged::Clean(bytes) => Ok(Some(bytes)),
                crate::merge::Merged::Conflicted { hunks } => Err(format!(
                    "`{path}` has {hunks} overlapping compose deletion and generator edit{}\n       fix: restore or reconcile that marked service by hand; nothing was written",
                    if hunks == 1 { "" } else { "s" }
                )),
            }
        }
        (Some(base), Some(current), None) if current == base => Ok(None),
        (Some(_), Some(_), None) => Err(format!(
            "`{path}` contains a hand-edited generated compose service that the model removes\n       fix: move the custom service outside the managed markers or restore the capability; nothing was written"
        )),
        (Some(_), None, None) | (None, None, None) => Ok(None),
        (None, Some(_), None) => Err(format!(
            "`{path}` contains an untracked generated compose facet\n       fix: restore `.jails/compiler.lock.json` or remove the stale marked block"
        )),
    }
}

fn compose_marked_range(text: &str, marker: &str) -> Result<Option<(usize, usize)>, String> {
    // The same two strings `codemod` writes, from `codemod`: this used to
    // build them here, so the file that finds a block and the file that writes
    // one were two statements of one format.
    let marked = jails_codemod::Marked::new(marker);
    let open = marked.open();
    let close = marked.close();
    let opens = line_ranges(text)
        .filter(|(start, end)| text[*start..*end].trim() == open)
        .collect::<Vec<_>>();
    let closes = line_ranges(text)
        .filter(|(start, end)| text[*start..*end].trim() == close)
        .collect::<Vec<_>>();
    match (opens.as_slice(), closes.as_slice()) {
        ([], []) => Ok(None),
        ([(start, _)], [(_, end)]) if start < end => Ok(Some((*start, *end))),
        _ => Err(format!(
            "compose marker `jails:{marker}` is missing, duplicated, or out of order\n       fix: keep exactly one opening and closing marker, then re-plan"
        )),
    }
}

fn line_ranges(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1);
        let range = (start, end);
        start = end;
        Some(range)
    })
}

fn compose_has_service(text: &str, service: &str) -> bool {
    let Some((_, body)) = text.split_once("services:") else {
        return false;
    };
    let body = body
        .split_once('\n')
        .map(|(_, body)| body)
        .unwrap_or_default();
    let expected = format!("  {service}:");
    body.lines()
        .take_while(|line| {
            line.is_empty() || line.starts_with(' ') || line.trim_start().starts_with('#')
        })
        .any(|line| line.trim_end() == expected)
}

fn insert_compose_service(text: &str, block: &str) -> String {
    if text.trim().is_empty() {
        return format!("{COMPOSE_HEADER}services:\n{block}");
    }
    if let Some(header_start) = top_level_header(text, "services") {
        let insert_at = next_top_level(text, header_start).unwrap_or(text.len());
        let mut output = String::with_capacity(text.len() + block.len() + 1);
        output.push_str(&text[..insert_at]);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(block);
        output.push_str(&text[insert_at..]);
        return output;
    }
    let separator = if text.ends_with('\n') { "" } else { "\n" };
    format!("{text}{separator}services:\n{block}")
}

fn top_level_header(text: &str, key: &str) -> Option<usize> {
    let expected = format!("{key}:");
    line_ranges(text).find_map(|(start, end)| {
        (text[start..end].trim_end_matches(['\r', '\n']) == expected).then_some(start)
    })
}

fn next_top_level(text: &str, header_start: usize) -> Option<usize> {
    line_ranges(text)
        .skip_while(|(start, _)| *start <= header_start)
        .find_map(|(start, end)| {
            let line = text[start..end].trim_end_matches(['\r', '\n']);
            (!line.is_empty() && !line.starts_with(' ') && !line.starts_with('#')).then_some(start)
        })
}

fn clean_empty_compose(text: String) -> String {
    if compose_service_count(&text) != 0 {
        return text;
    }
    let without_services = if let Some(start) = top_level_header(&text, "services") {
        let end = next_top_level(&text, start).unwrap_or(text.len());
        format!("{}{}", &text[..start], &text[end..])
    } else {
        text
    };
    let reader = without_services
        .strip_prefix(COMPOSE_HEADER)
        .unwrap_or(&without_services);
    if reader.trim().is_empty() {
        String::new()
    } else {
        without_services
    }
}

fn compose_service_count(text: &str) -> usize {
    let Some(start) = top_level_header(text, "services") else {
        return 0;
    };
    let end = next_top_level(text, start).unwrap_or(text.len());
    text[start..end]
        .lines()
        .skip(1)
        .filter(|line| {
            line.starts_with("  ")
                && !line.starts_with("    ")
                && !line.trim_start().starts_with('#')
                && line.trim_end().ends_with(':')
        })
        .count()
}

fn property_key(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    let candidate = line.trim_start();
    if candidate.is_empty() || candidate.starts_with('#') || candidate.starts_with('!') {
        return None;
    }
    let end = candidate
        .bytes()
        .position(|byte| matches!(byte, b'=' | b':' | b' ' | b'\t'))
        .unwrap_or(candidate.len());
    (end > 0).then_some(&candidate[..end])
}

fn has_continuation(line: &str) -> bool {
    line.trim_end_matches(['\r', '\n'])
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count()
        % 2
        == 1
}

fn escape_property_value(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        match character {
            '\\' => output.push_str("\\\\"),
            ' ' if index == 0 => output.push_str("\\ "),
            other => output.push(other),
        }
    }
    output
}

pub(crate) fn reconcile_maven_dependencies(
    text: &str,
    dependencies: &[jails_contracts::BuildDependency],
) -> Result<String, String> {
    let open = format!("<!-- {DEPENDENCY_MARKER} -->");
    let close = format!("<!-- /{DEPENDENCY_MARKER} -->");
    let body = maven_dependency_block(dependencies);
    if let Some(replaced) = replace_owned_block(text, &open, &close, body.as_deref())? {
        return Ok(replaced);
    }
    let Some(block) = body else {
        return Ok(text.to_string());
    };
    refuse_unowned_maven_duplicates(text, dependencies)?;
    if let Some(at) = direct_child_close(text, &["project", "dependencies"]) {
        return Ok(insert_indented_block(text, at, &block, 0));
    }
    let Some(at) = direct_child_close(text, &["project"]) else {
        return Err(
            "pom.xml has no closing project element\n       fix: repair the Maven POM, then re-plan"
                .to_string(),
        );
    };
    let indent = line_indent(text, at).unwrap_or("");
    let child = format!("{indent}    ");
    let section = format!(
        "{child}<dependencies>\n{}{child}</dependencies>\n",
        indent_block(&block, &format!("{child}    "))
    );
    Ok(insert_at_line(text, at, &section))
}

pub(crate) fn reconcile_gradle_dependencies(
    text: &str,
    dependencies: &[jails_contracts::BuildDependency],
    kotlin: bool,
) -> Result<String, String> {
    let open = format!("// {DEPENDENCY_MARKER}");
    let close = format!("// /{DEPENDENCY_MARKER}");
    let body = gradle_dependency_block(dependencies, kotlin);
    if let Some(replaced) = replace_owned_block(text, &open, &close, body.as_deref())? {
        return Ok(replaced);
    }
    let Some(block) = body else {
        return Ok(text.to_string());
    };
    for dependency in dependencies {
        let coordinate = format!("{}:{}", dependency.group, dependency.artifact);
        if text.contains(&coordinate) {
            return Err(format!(
                "Gradle already declares `{coordinate}` outside `{open}`\n       fix: remove the reader-owned duplicate or declare it only in the canonical model"
            ));
        }
    }
    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(format!("{text}{separator}\n{block}"))
}

fn maven_dependency_block(dependencies: &[jails_contracts::BuildDependency]) -> Option<String> {
    if dependencies.is_empty() {
        return None;
    }
    let mut block = format!("<!-- {DEPENDENCY_MARKER} -->\n");
    for dependency in dependencies {
        block.push_str("<dependency>\n");
        block.push_str(&format!("    <groupId>{}</groupId>\n", dependency.group));
        block.push_str(&format!(
            "    <artifactId>{}</artifactId>\n",
            dependency.artifact
        ));
        if let Some(version) = &dependency.version {
            block.push_str(&format!("    <version>{version}</version>\n"));
        }
        block.push_str(&format!(
            "    <scope>{}</scope>\n",
            dependency_scope(dependency.scope)
        ));
        block.push_str("</dependency>\n");
    }
    block.push_str(&format!("<!-- /{DEPENDENCY_MARKER} -->\n"));
    Some(block)
}

fn gradle_dependency_block(
    dependencies: &[jails_contracts::BuildDependency],
    kotlin: bool,
) -> Option<String> {
    if dependencies.is_empty() {
        return None;
    }
    let mut block = format!("// {DEPENDENCY_MARKER}\ndependencies {{\n");
    for dependency in dependencies {
        let coordinate = match &dependency.version {
            Some(version) => format!("{}:{}:{version}", dependency.group, dependency.artifact),
            None => format!("{}:{}", dependency.group, dependency.artifact),
        };
        let configuration = match dependency.scope {
            jails_model::DependencyScope::Compile => "implementation",
            jails_model::DependencyScope::Runtime => "runtimeOnly",
            jails_model::DependencyScope::Test => "testImplementation",
        };
        if kotlin {
            block.push_str(&format!("    {configuration}(\"{coordinate}\")\n"));
        } else {
            block.push_str(&format!("    {configuration} '{coordinate}'\n"));
        }
    }
    block.push_str(&format!("}}\n// /{DEPENDENCY_MARKER}\n"));
    Some(block)
}

fn dependency_scope(scope: jails_model::DependencyScope) -> &'static str {
    match scope {
        jails_model::DependencyScope::Compile => "compile",
        jails_model::DependencyScope::Runtime => "runtime",
        jails_model::DependencyScope::Test => "test",
    }
}

fn refuse_unowned_maven_duplicates(
    text: &str,
    dependencies: &[jails_contracts::BuildDependency],
) -> Result<(), String> {
    for dependency in dependencies {
        let group = format!("<groupId>{}</groupId>", dependency.group);
        let artifact = format!("<artifactId>{}</artifactId>", dependency.artifact);
        if text.contains(&group) && text.contains(&artifact) {
            let coordinate = format!("{}:{}", dependency.group, dependency.artifact);
            return Err(format!(
                "Maven already declares `{coordinate}` outside `<!-- {DEPENDENCY_MARKER} -->`\n       fix: remove the reader-owned duplicate or declare it only in the canonical model"
            ));
        }
    }
    Ok(())
}

fn replace_owned_block(
    text: &str,
    open: &str,
    close: &str,
    replacement: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(block) = owned_block(text, open, close)? else {
        return Ok(None);
    };
    let marker_start = block.as_ptr() as usize - text.as_ptr() as usize;
    let line_start = text[..marker_start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let indent = &text[line_start..marker_start];
    let marker_end = marker_start + block.len();
    let end = marker_end + usize::from(text.as_bytes().get(marker_end) == Some(&b'\n'));
    let mut output = String::with_capacity(text.len() + replacement.map_or(0, str::len));
    output.push_str(&text[..line_start]);
    if let Some(replacement) = replacement {
        output.push_str(&indent_block(replacement, indent));
    }
    output.push_str(&text[end..]);
    Ok(Some(output))
}

fn owned_block<'a>(text: &'a str, open: &str, close: &str) -> Result<Option<&'a str>, String> {
    let Some(start) = text.find(open) else {
        if text.contains(close) {
            return Err(format!(
                "found `{close}` without its opening marker\n       fix: repair or remove the damaged owned block, then re-plan"
            ));
        }
        return Ok(None);
    };
    let Some(relative_end) = text[start..].find(close) else {
        return Err(format!(
            "found `{open}` without its closing marker\n       fix: repair or remove the damaged owned block, then re-plan"
        ));
    };
    let end = start + relative_end + close.len();
    Ok(Some(&text[start..end]))
}

#[derive(Debug)]
struct Tag {
    name: String,
    start: usize,
    closing: bool,
    self_closing: bool,
}

fn scan_tags(xml: &str) -> Vec<Tag> {
    let bytes = xml.as_bytes();
    let mut tags = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] != b'<' {
            offset += 1;
            continue;
        }
        let rest = &xml[offset..];
        if rest.starts_with("<!--") {
            offset += rest.find("-->").map_or(rest.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            offset += rest.find("]]>").map_or(rest.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            offset += rest.find('>').map_or(rest.len(), |end| end + 1);
            continue;
        }
        let Some(end) = rest.find('>') else {
            break;
        };
        let inner = &rest[1..end];
        let closing = inner.starts_with('/');
        let self_closing = inner.trim_end().ends_with('/');
        let name = inner
            .trim_start_matches('/')
            .trim_start()
            .chars()
            .take_while(|character| !character.is_whitespace() && *character != '/')
            .collect::<String>();
        if !name.is_empty() {
            tags.push(Tag {
                name,
                start: offset,
                closing,
                self_closing,
            });
        }
        offset += end + 1;
    }
    tags
}

fn direct_child_close(xml: &str, target: &[&str]) -> Option<usize> {
    let mut stack = Vec::<String>::new();
    for tag in scan_tags(xml) {
        if tag.closing {
            if stack.iter().map(String::as_str).eq(target.iter().copied())
                && stack.last().is_some_and(|name| name == &tag.name)
            {
                return Some(tag.start);
            }
            stack.pop();
        } else if !tag.self_closing {
            stack.push(tag.name);
        }
    }
    None
}

fn insert_indented_block(text: &str, at: usize, block: &str, extra: usize) -> String {
    let parent = line_indent(text, at).unwrap_or("");
    let indent = format!("{parent}{}", "    ".repeat(extra + 1));
    insert_at_line(text, at, &indent_block(block, &indent))
}

fn insert_at_line(text: &str, at: usize, block: &str) -> String {
    let line = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let mut output = String::with_capacity(text.len() + block.len());
    output.push_str(&text[..line]);
    output.push_str(block);
    output.push_str(&text[line..]);
    output
}

fn indent_block(block: &str, indent: &str) -> String {
    let mut output = String::with_capacity(block.len() + indent.len() * 16);
    for line in block.trim_end().lines() {
        if !line.is_empty() {
            output.push_str(indent);
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn line_indent(text: &str, at: usize) -> Option<&str> {
    let line = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let prefix = &text[line..at];
    prefix
        .chars()
        .all(|character| matches!(character, ' ' | '\t'))
        .then_some(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_contracts::ProjectPath;

    fn dependency(
        group: &str,
        artifact: &str,
        version: Option<&str>,
        scope: jails_model::DependencyScope,
    ) -> jails_contracts::BuildDependency {
        jails_contracts::BuildDependency {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.map(str::to_string),
            scope,
        }
    }

    fn property(key: &str, value: &str) -> jails_contracts::PropertyEntry {
        jails_contracts::PropertyEntry {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn maven_dependencies_are_one_reconciled_owned_set() {
        let pom = "<project>\n    <!-- reader -->\n</project>\n";
        let first = dependency(
            "org.jsoup",
            "jsoup",
            Some("1.18.3"),
            jails_model::DependencyScope::Runtime,
        );
        let once = reconcile_maven_dependencies(pom, std::slice::from_ref(&first)).unwrap();
        assert!(once.contains("<!-- reader -->"));
        assert!(once.contains("<!-- jails:dependencies -->"));
        assert!(once.contains("<artifactId>jsoup</artifactId>"));
        assert!(once.contains("<scope>runtime</scope>"));
        assert_eq!(
            reconcile_maven_dependencies(&once, std::slice::from_ref(&first)).unwrap(),
            once
        );
        let removed = reconcile_maven_dependencies(&once, &[]).unwrap();
        assert!(removed.contains("<!-- reader -->"));
        assert!(!removed.contains("jails:dependencies"));
        assert!(!removed.contains("<artifactId>jsoup</artifactId>"));
    }

    #[test]
    fn gradle_dependencies_use_the_build_dialect_and_can_be_removed() {
        let dependency = dependency(
            "org.jsoup",
            "jsoup",
            Some("1.18.3"),
            jails_model::DependencyScope::Test,
        );
        let groovy = reconcile_gradle_dependencies(
            "plugins { id 'java' }\n",
            std::slice::from_ref(&dependency),
            false,
        )
        .unwrap();
        assert!(groovy.contains("testImplementation 'org.jsoup:jsoup:1.18.3'"));
        let kotlin = reconcile_gradle_dependencies(
            "plugins { java }\n",
            std::slice::from_ref(&dependency),
            true,
        )
        .unwrap();
        assert!(kotlin.contains("testImplementation(\"org.jsoup:jsoup:1.18.3\")"));
        let removed = reconcile_gradle_dependencies(&groovy, &[], false).unwrap();
        assert!(removed.contains("plugins { id 'java' }"));
        assert!(!removed.contains("jails:dependencies"));
        assert!(!removed.contains("org.jsoup:jsoup"));
    }

    #[test]
    fn properties_reconcile_owned_keys_and_preserve_reader_bytes() {
        let text = "# reader\nreader.key=keep\nserver.port=8080\n";
        let next = reconcile_properties(
            text,
            &[property("server.port", "8080")],
            &[
                property("server.port", "9090"),
                property("spring.threads.virtual.enabled", "true"),
            ],
        )
        .unwrap();
        assert!(next.starts_with("# reader\nreader.key=keep\n"));
        assert!(next.contains("server.port=9090\n"));
        assert!(next.contains("spring.threads.virtual.enabled=true\n"));
        let removed = reconcile_properties(
            &next,
            &[
                property("server.port", "9090"),
                property("spring.threads.virtual.enabled", "true"),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(removed, "# reader\nreader.key=keep\n");
    }

    #[test]
    fn properties_refuse_to_claim_a_reader_key() {
        let error = reconcile_properties(
            "server.port=7000\n",
            &[],
            &[property("server.port", "8080")],
        )
        .unwrap_err();
        assert!(error.contains("reader-owned"), "{error}");
        assert!(error.contains("fix:"), "{error}");
    }

    #[test]
    fn compose_service_uses_three_way_merge_inside_a_lossless_reader_document() {
        let path = ProjectPath::parse("compose.yaml").unwrap();
        let base = b"  # jails:redis\n  redis:\n    image: redis:7-alpine\n    healthcheck:\n      retries: 10\n  # /jails:redis\n";
        let initial =
            reconcile_compose_service(&path, "", "redis", "redis", None, Some(base)).unwrap();
        assert!(initial.starts_with(COMPOSE_HEADER));
        assert!(initial.contains("services:\n"));

        let reader = initial
            .replace(
                "    healthcheck:\n",
                "    restart: unless-stopped\n    healthcheck:\n",
            )
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader\n",
            );
        let desired = b"  # jails:redis\n  redis:\n    image: redis:7-alpine\n    healthcheck:\n      retries: 12\n  # /jails:redis\n";
        let merged =
            reconcile_compose_service(&path, &reader, "redis", "redis", Some(base), Some(desired))
                .unwrap();
        assert!(merged.contains("restart: unless-stopped"), "{merged}");
        assert!(merged.contains("retries: 12"), "{merged}");
        assert!(
            merged.contains("reader-service:\n    image: reader"),
            "{merged}"
        );

        let removed =
            reconcile_compose_service(&path, &merged, "redis", "redis", Some(desired), None);
        assert!(
            removed.is_err(),
            "edited generated service was silently removed"
        );
    }

    #[test]
    fn compose_service_refuses_overlapping_generated_line_edits() {
        let path = ProjectPath::parse("compose.yaml").unwrap();
        let base = b"  # jails:redis\n  redis:\n    image: redis:7-alpine\n  # /jails:redis\n";
        let reader = b"  # jails:redis\n  redis:\n    image: redis:reader\n  # /jails:redis\n";
        let desired = b"  # jails:redis\n  redis:\n    image: redis:generator\n  # /jails:redis\n";
        let error =
            reconcile_facet_bytes(&path, Some(base), Some(reader), Some(desired)).unwrap_err();
        assert!(error.contains("overlapping compose edit"), "{error}");
        assert!(error.contains("nothing was written"), "{error}");
    }
}
