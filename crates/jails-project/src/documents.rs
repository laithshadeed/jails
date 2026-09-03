//! Bounded, lossless edits to reader-owned build documents.
//!
//! These adapters do not try to understand Maven or Gradle as languages. They
//! insert one explicitly owned source-root block, preserve every other byte,
//! and refuse a damaged/edited owned block instead of guessing.
//!
//! [`pom`] is the one reader of Maven's XML in the workspace -- what every
//! adapter here asks where a block goes, and what capture asks what the build
//! declares.

mod build_feature;
pub mod pom;
mod relocate;
mod spring_test;

pub use build_feature::{reconcile_gradle_build_features, reconcile_maven_build_features};
// The one walk of a Maven POM, so the adapters here and the capture beside
// them agree about where an element begins and ends.
pub(crate) use pom::direct_child_close;
pub use relocate::strip_generated_source_roots;
pub use spring_test::{
    command_dispatcher, ensure_command_registration, ensure_spring_test_import,
    remove_spring_test_import, set_maven_main_class, spring_boot_test_targets,
};

use jails_model::Diagnostic;

pub(crate) const DEPENDENCY_MARKER: &str = "jails:dependencies";

/// The build file these adapters refuse about when no path is in scope.
///
/// A dependency or a build feature is reconciled into whichever build file the
/// project has, and the adapter is handed the text rather than the path. The
/// diagnostic's subject is the build, spelled the way the model spells one.
const BUILD_SUBJECT: &str = "$.build";

pub fn reconcile_properties(
    text: &str,
    previous: &[jails_contracts::PropertyEntry],
    desired: &[jails_contracts::PropertyEntry],
) -> Result<String, Diagnostic> {
    use jails_codemod::Marked;
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
    // **A `# jails:<name>` block is jails' own, however old.** The compiler
    // claims a key at a time and has no markers, but a project can arrive
    // with keys jails wrote inside markers jails wrote -- and reading those as
    // the reader's would refuse `add db` permanently, with a fix line telling
    // the reader to delete a line they never wrote. The block is adopted and
    // its markers dissolve.
    let mut inside_marker = false;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with(Marked::OPEN_PREFIX) {
            inside_marker = true;
            continue;
        }
        if trimmed.starts_with(Marked::CLOSE_PREFIX) {
            inside_marker = false;
            continue;
        }
        let Some(key) = property_key(line) else {
            output.push_str(line);
            continue;
        };
        if !owned.contains(key) {
            output.push_str(line);
            continue;
        }
        if !seen.insert(key.to_string()) {
            return Err(Diagnostic::new(
                "workspace-properties-key-repeated",
                key.to_string(),
                format!("properties key `{key}` occurs more than once"),
                "keep one declaration for the key, then re-plan",
            ));
        }
        if !previous.contains_key(key) && !inside_marker {
            return Err(Diagnostic::new(
                "workspace-properties-key-reader-owned",
                key.to_string(),
                format!("reader-owned properties already declare `{key}`"),
                "remove the reader-owned key or do not declare it in the canonical model",
            ));
        }
        if has_continuation(line) {
            return Err(Diagnostic::new(
                "workspace-properties-continuation",
                key.to_string(),
                format!("managed properties key `{key}` uses a continuation line"),
                "rewrite the value on one line, then re-plan",
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
pub fn reconcile_compose_service(
    path: &jails_contracts::ProjectPath,
    text: &str,
    service: &str,
    marker: &str,
    previous: Option<&[u8]>,
    desired: Option<&[u8]>,
) -> Result<String, Diagnostic> {
    let range = compose_marked_range(text, marker)?;
    let current = range.map(|(start, end)| &text.as_bytes()[start..end]);
    if previous.is_none() && range.is_none() && compose_has_service(text, service) {
        return Err(Diagnostic::new(
            "workspace-compose-service-unmarked",
            path.to_string(),
            format!(
                "compose service `{service}` already exists outside `{}{marker}`",
                jails_codemod::Marked::OPEN_PREFIX
            ),
            "rename the reader-owned service or remove the canonical capability",
        ));
    }
    let selected = reconcile_facet_bytes(path, previous, current, desired)?;
    match (range, selected) {
        (Some((start, end)), Some(bytes)) => {
            let replacement =
                std::str::from_utf8(&bytes).map_err(|_| non_utf8_compose_facet(path, service))?;
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
            let block =
                std::str::from_utf8(&bytes).map_err(|_| non_utf8_compose_facet(path, service))?;
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
) -> Result<Option<Vec<u8>>, Diagnostic> {
    match (base, current, desired) {
        (None, None, Some(desired)) => Ok(Some(desired.to_vec())),
        (None, Some(_), Some(_)) => Err(Diagnostic::new(
            "workspace-compose-facet-no-base",
            path.to_string(),
            format!("generated compose facet in `{path}` has no accepted merge base"),
            "restore `.jails/compiler.lock.json` or move the colliding marked block",
        )),
        (Some(base), Some(current), Some(desired)) if current == base => Ok(Some(desired.to_vec())),
        (Some(base), Some(current), Some(desired)) if desired == base => Ok(Some(current.to_vec())),
        (Some(_), Some(current), Some(desired)) if current == desired => Ok(Some(desired.to_vec())),
        (Some(base), Some(current), Some(desired)) => {
            match crate::merge::three_way(path, base, current, desired)? {
                crate::merge::Merged::Clean(bytes) => Ok(Some(bytes)),
                crate::merge::Merged::Conflicted { hunks } => Err(Diagnostic::new(
                    "workspace-compose-facet-conflict",
                    path.to_string(),
                    format!(
                        "`{path}` has {hunks} overlapping compose edit{} between your service and the generator",
                        if hunks == 1 { "" } else { "s" }
                    ),
                    "reconcile that marked service by hand; nothing was written",
                )),
            }
        }
        (Some(base), None, Some(desired)) if base == desired => Ok(None),
        (Some(base), None, Some(desired)) => {
            match crate::merge::three_way(path, base, b"", desired)? {
                crate::merge::Merged::Clean(bytes) if bytes.is_empty() => Ok(None),
                crate::merge::Merged::Clean(bytes) => Ok(Some(bytes)),
                crate::merge::Merged::Conflicted { hunks } => Err(Diagnostic::new(
                    "workspace-compose-facet-deletion-conflict",
                    path.to_string(),
                    format!(
                        "`{path}` has {hunks} overlapping compose deletion and generator edit{}",
                        if hunks == 1 { "" } else { "s" }
                    ),
                    "restore or reconcile that marked service by hand; nothing was written",
                )),
            }
        }
        (Some(base), Some(current), None) if current == base => Ok(None),
        (Some(_), Some(_), None) => Err(Diagnostic::new(
            "workspace-compose-facet-edited-and-removed",
            path.to_string(),
            format!(
                "`{path}` contains a hand-edited generated compose service that the model removes"
            ),
            "move the custom service outside the managed markers or restore the capability; nothing was written",
        )),
        (Some(_), None, None) | (None, None, None) => Ok(None),
        (None, Some(_), None) => Err(Diagnostic::new(
            "workspace-compose-facet-untracked",
            path.to_string(),
            format!("`{path}` contains an untracked generated compose facet"),
            "restore `.jails/compiler.lock.json` or remove the stale marked block",
        )),
    }
}

/// A POM with no `</project>`. One site for the two adapters that insert
/// into one: the dependency block here and the plugin block in [`pom`].
pub(crate) fn maven_project_unclosed() -> Diagnostic {
    Diagnostic::new(
        "workspace-maven-project-unclosed",
        "pom.xml",
        "pom.xml has no closing project element",
        "repair the Maven POM, then re-plan",
    )
}

/// A compose block the compiler produced that is not text. One site, because
/// the replace and the insert arms reach the same refusal.
fn non_utf8_compose_facet(path: &jails_contracts::ProjectPath, service: &str) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-compose-facet-not-utf8",
        path.to_string(),
        format!("compiler emitted non-UTF-8 compose facet for `{service}`"),
    )
}

fn compose_marked_range(text: &str, marker: &str) -> Result<Option<(usize, usize)>, Diagnostic> {
    // The same two strings `codemod` writes, from `codemod`. Building them
    // here makes the file that finds a block and the file that writes one two
    // statements of one format.
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
        _ => Err(Diagnostic::new(
            "workspace-compose-marker-damaged",
            format!("jails:{marker}"),
            format!("compose marker `jails:{marker}` is missing, duplicated, or out of order"),
            "keep exactly one opening and closing marker, then re-plan",
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

pub fn reconcile_maven_dependencies(
    text: &str,
    dependencies: &[jails_contracts::BuildDependency],
) -> Result<String, Diagnostic> {
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
        return Err(maven_project_unclosed());
    };
    let indent = line_indent(text, at).unwrap_or("");
    let child = format!("{indent}    ");
    let section = format!(
        "{child}<dependencies>\n{}{child}</dependencies>\n",
        indent_block(&block, &format!("{child}    "))
    );
    Ok(insert_at_line(text, at, &section))
}

pub fn reconcile_gradle_dependencies(
    text: &str,
    dependencies: &[jails_contracts::BuildDependency],
    kotlin: bool,
) -> Result<String, Diagnostic> {
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
        if declares_coordinate(text, &coordinate) {
            return Err(Diagnostic::new(
                "workspace-gradle-dependency-reader-owned",
                BUILD_SUBJECT,
                format!("Gradle already declares `{coordinate}` outside `{open}`"),
                "remove the reader-owned duplicate or declare it only in the canonical model",
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

/// The exact `<!-- jails:dependencies -->` block the canonical plan renders.
///
/// Public because `jails new` writes a pom whose dependency block the compiler
/// will own from the project's first canonical command. Rendering it here
/// rather than in `new` is what keeps that first command a no-op instead of a
/// rewrite: two spellings of this block differ in whitespace or scope long
/// before anyone notices, and the difference shows up as a surprise diff in a
/// file the reader believes jails has not touched yet.
pub fn maven_dependency_block(dependencies: &[jails_contracts::BuildDependency]) -> Option<String> {
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
        if dependency.optional {
            block.push_str("    <optional>true</optional>\n");
        }
        block.push_str("</dependency>\n");
    }
    block.push_str(&format!("<!-- /{DEPENDENCY_MARKER} -->\n"));
    Some(block)
}

/// Whether the build already declares this exact `group:artifact`.
///
/// **The whole coordinate, bounded by what may follow it.** A bare
/// `text.contains("group:artifact")` matches every coordinate that *starts*
/// with one, and Spring ships a family that does: a build declaring
/// `org.springframework.boot:spring-boot-starter-websocket` would refuse the
/// required `...:spring-boot-starter-web`, naming a coordinate the build does
/// not have. `-webflux` and `-webmvc` are the same trap.
///
/// A Gradle coordinate is followed by a version separator or by the quote that
/// closes the string, so those are the only characters that end one. Anything
/// else means the match landed inside a longer artifact name.
fn declares_coordinate(text: &str, coordinate: &str) -> bool {
    text.match_indices(coordinate).any(|(at, _)| {
        text[at + coordinate.len()..]
            .chars()
            .next()
            .is_none_or(|next| matches!(next, ':' | '\'' | '"'))
    })
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
        // `BuildDependency::optional` needs no rendering here: every
        // configuration below is already non-transitive for a consumer's
        // compile classpath, which is the whole of what Maven's
        // `<optional>true</optional>` buys. `developmentOnly` would say
        // something else -- keep it out of the artifact entirely -- and exists
        // only once the Spring Boot Gradle plugin is applied.
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
    // **The block carries jails' classpath task beside the dependencies it
    // declares.** The warm test engine, `jails console` and `jails runner`
    // need the classpath the build resolves and the directories it writes,
    // and on Gradle the only exact answer is the build's own -- so the
    // question is registered as a task in the one block jails owns, and a
    // build without the block is refused by name rather than read for a
    // layout. See `gradle::classpath_task`.
    block.push_str("}\n\n");
    block.push_str(&crate::gradle::classpath_task(kotlin));
    block.push_str(&format!("// /{DEPENDENCY_MARKER}\n"));
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
) -> Result<(), Diagnostic> {
    for dependency in dependencies {
        let group = format!("<groupId>{}</groupId>", dependency.group);
        let artifact = format!("<artifactId>{}</artifactId>", dependency.artifact);
        if text.contains(&group) && text.contains(&artifact) {
            let coordinate = format!("{}:{}", dependency.group, dependency.artifact);
            return Err(Diagnostic::new(
                "workspace-maven-dependency-reader-owned",
                "pom.xml",
                format!(
                    "Maven already declares `{coordinate}` outside `<!-- {DEPENDENCY_MARKER} -->`"
                ),
                "remove the reader-owned duplicate or declare it only in the canonical model",
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
) -> Result<Option<String>, Diagnostic> {
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

fn owned_block<'a>(text: &'a str, open: &str, close: &str) -> Result<Option<&'a str>, Diagnostic> {
    let Some(start) = text.find(open) else {
        if text.contains(close) {
            return Err(Diagnostic::new(
                "workspace-owned-block-unopened",
                BUILD_SUBJECT,
                format!("found `{close}` without its opening marker"),
                "repair or remove the damaged owned block, then re-plan",
            ));
        }
        return Ok(None);
    };
    let Some(relative_end) = text[start..].find(close) else {
        return Err(Diagnostic::new(
            "workspace-owned-block-unclosed",
            BUILD_SUBJECT,
            format!("found `{open}` without its closing marker"),
            "repair or remove the damaged owned block, then re-plan",
        ));
    };
    let end = start + relative_end + close.len();
    Ok(Some(&text[start..end]))
}

fn insert_indented_block(text: &str, at: usize, block: &str, extra: usize) -> String {
    let parent = line_indent(text, at).unwrap_or("");
    let indent = format!("{parent}{}", "    ".repeat(extra + 1));
    insert_at_line(text, at, &indent_block(block, &indent))
}

/// Insert `block` immediately before the element that closes at `at`.
///
/// Normally that means the start of `at`'s line, so the inserted block lands
/// on its own lines with the closing tag's indentation intact. **Only when
/// that line holds nothing but whitespace before `at`** -- which is the case
/// for every pom anyone formats, and is what makes the indentation correct.
///
/// When it does not, the block goes at `at` exactly. Otherwise a reader whose
/// `<dependencies><dependency>...</dependency></dependencies>` is one line
/// gets the block inserted before the *whole element*, outside
/// `<dependencies>`, and Maven then refuses to read the pom at all: every
/// goal fails, `validate` included, and the project is worse off than before
/// the command ran.
fn insert_at_line(text: &str, at: usize, block: &str) -> String {
    let line = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let at = if text[line..at].trim().is_empty() {
        line
    } else {
        at
    };
    let mut output = String::with_capacity(text.len() + block.len());
    output.push_str(&text[..at]);
    output.push_str(block);
    output.push_str(&text[at..]);
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
            optional: false,
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

    /// The block carries jails' classpath task beside the dependencies, in
    /// the build's dialect, and leaves with them: a Gradle project the model
    /// declares nothing into has no task and no block, and is refused by name
    /// when asked for a classpath rather than read for a layout.
    #[test]
    fn the_gradle_block_carries_the_classpath_task_and_removes_it_with_the_dependencies() {
        let dependency = dependency(
            "org.junit.platform",
            "junit-platform-console",
            None,
            jails_model::DependencyScope::Test,
        );
        let groovy = reconcile_gradle_dependencies(
            "plugins { id 'java' }\n",
            std::slice::from_ref(&dependency),
            false,
        )
        .unwrap();
        assert!(
            groovy.contains("tasks.register('jailsClasspath')"),
            "{groovy}"
        );
        assert!(
            groovy.contains("configurations.testRuntimeClasspath"),
            "{groovy}"
        );
        assert!(crate::gradle::declares_classpath_task(&groovy));
        // Inside the markers, so the reader's bytes are untouched and the
        // whole thing is one owned block.
        let open = groovy.find("// jails:dependencies").unwrap();
        let close = groovy.find("// /jails:dependencies").unwrap();
        let task = groovy.find("tasks.register('jailsClasspath')").unwrap();
        assert!(open < task && task < close, "{groovy}");
        // A second plan renders the same block: idempotent by bytes.
        assert_eq!(
            reconcile_gradle_dependencies(&groovy, std::slice::from_ref(&dependency), false)
                .unwrap(),
            groovy
        );

        let kotlin = reconcile_gradle_dependencies(
            "plugins { java }\n",
            std::slice::from_ref(&dependency),
            true,
        )
        .unwrap();
        assert!(
            kotlin.contains("tasks.register(\"jailsClasspath\")"),
            "{kotlin}"
        );
        assert!(kotlin.contains("configurations[\"testRuntimeClasspath\"]"));
        assert!(crate::gradle::declares_classpath_task(&kotlin));

        let removed = reconcile_gradle_dependencies(&groovy, &[], false).unwrap();
        assert!(!removed.contains("jailsClasspath"), "{removed}");
        assert!(!crate::gradle::declares_classpath_task(&removed));
    }

    /// A reader-owned duplicate is refused by whole coordinate, not by
    /// prefix. Spring ships a family of them -- `-web`, `-webmvc`, `-webflux`,
    /// `-websocket` -- so a substring match refuses a build for a coordinate
    /// it does not have, and names one that is nowhere in the file.
    #[test]
    fn a_longer_gradle_coordinate_is_not_read_as_the_one_it_starts_with() {
        let build = "plugins { id 'java' }\n\ndependencies {\n    implementation 'org.springframework.boot:spring-boot-starter-websocket'\n}\n";
        let required = dependency(
            "org.springframework.boot",
            "spring-boot-starter-web",
            None,
            jails_model::DependencyScope::Compile,
        );
        let next =
            reconcile_gradle_dependencies(build, std::slice::from_ref(&required), false).unwrap();
        assert!(
            next.contains("implementation 'org.springframework.boot:spring-boot-starter-web'"),
            "{next}"
        );
        assert!(next.contains("spring-boot-starter-websocket"), "{next}");

        // The real duplicate still refuses, in both dialects.
        let declared = "plugins { id 'java' }\n\ndependencies {\n    implementation 'org.springframework.boot:spring-boot-starter-web'\n}\n";
        assert!(
            reconcile_gradle_dependencies(declared, std::slice::from_ref(&required), false)
                .is_err()
        );
        let kotlin = "plugins { java }\n\ndependencies {\n    implementation(\"org.springframework.boot:spring-boot-starter-web\")\n}\n";
        assert!(
            reconcile_gradle_dependencies(kotlin, std::slice::from_ref(&required), true).is_err()
        );
        let versioned = "plugins { id 'java' }\n\ndependencies {\n    implementation 'org.springframework.boot:spring-boot-starter-web:4.1.0'\n}\n";
        assert!(
            reconcile_gradle_dependencies(versioned, std::slice::from_ref(&required), false)
                .is_err()
        );
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
        assert!(error.to_string().contains("reader-owned"), "{error}");
        assert!(error.to_string().contains("fix:"), "{error}");
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
        assert!(
            error.to_string().contains("overlapping compose edit"),
            "{error}"
        );
        assert!(error.to_string().contains("nothing was written"), "{error}");
    }
}

#[cfg(test)]
mod single_line_parent_tests {
    use super::*;

    /// A reader whose `<dependencies>` is one line still gets a readable pom.
    ///
    /// Putting the block at the start of the closing tag's *line* is right
    /// when the tag begins its own line and wrong when it does not: the
    /// dependencies land outside `<dependencies>`, Maven refuses to read the
    /// pom at all, and every goal fails including `validate`. It needs no
    /// unusual pom, only one nobody reformatted.
    #[test]
    fn a_one_line_dependencies_element_still_receives_its_block_inside() {
        let pom = concat!(
            "<project>\n",
            "<dependencies><dependency><groupId>a</groupId></dependency></dependencies>\n",
            "</project>\n"
        );
        let at = direct_child_close(pom, &["project", "dependencies"]).expect("the closing tag");
        let patched = insert_at_line(pom, at, "<dependency><groupId>b</groupId></dependency>");
        let opened = patched.find("<dependencies>").expect("the opening tag");
        let inserted = patched.find("<groupId>b</groupId>").expect("the new block");
        let closed = patched.find("</dependencies>").expect("the closing tag");
        assert!(
            opened < inserted && inserted < closed,
            "the block landed outside its parent, which Maven refuses to parse:\n{patched}"
        );
    }

    /// The ordinary case is unchanged: a closing tag on its own line keeps the
    /// block on its own lines, with the indentation that makes a pom readable.
    #[test]
    fn a_formatted_element_still_gets_the_block_on_its_own_lines() {
        let pom = "<project>\n    <dependencies>\n    </dependencies>\n</project>\n";
        let at = direct_child_close(pom, &["project", "dependencies"]).expect("the closing tag");
        let patched = insert_at_line(pom, at, "        <dependency/>\n");
        assert!(
            patched.contains("        <dependency/>\n    </dependencies>"),
            "{patched}"
        );
    }
}
