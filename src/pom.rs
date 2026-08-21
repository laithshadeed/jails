//! Reading and editing `pom.xml`.
//!
//! Everything else in jails only ever *creates* files (`write_new_file`
//! refuses to clobber). `add` is the first command that has to modify a file
//! the user owns and hand-edits, so the edit is a targeted splice rather than
//! an XML round-trip: locate the project-level `<dependencies>` element, and
//! insert text at that byte offset. Every other byte of the file -- comments,
//! formatting, attribute order -- is preserved exactly.
//!
//! A real XML crate would parse more correctly but reformat the whole
//! document on write, which is unacceptable for a file people maintain by
//! hand.

use crate::Result;
use std::path::Path;

/// The Java release every generated project targets by default. Referenced by
/// `new-cli`'s `--release` default, `new`'s `--java` default, and the tier-3
/// test gate.
///
/// **This is the one place the target is decided.** Three other files
/// describe the same toolchain and have to agree with it: `mise.toml` (which
/// JDK this machine installs) and `tests/common/mod.rs`'s `TARGET_RELEASE`
/// (the integration tests compile against the binary, not the library, so the
/// constant cannot be imported; `target_release_matches_the_binary` pins them
/// together).
///
/// Java 25 is the current LTS and the conservative production default. Newer
/// GA or early-access releases remain an explicit `--java`/`--release`
/// choice; the default must not make real-toolchain tests silently skip while
/// waiting for a future JDK to ship.
pub const TARGET_RELEASE: &str = "25";

/// The oldest release the *generated* Java actually needs. Everything jails
/// emits -- records, sealed interfaces, text blocks, pattern-matching switch,
/// `SequencedMap`, `List.getFirst()`, virtual threads -- went final in 21, so
/// that is the honest floor. `add` checks against this rather than
/// `TARGET_RELEASE`, so a project pinned to an older release than jails'
/// default is still one `add` can grow.
pub const MIN_RELEASE: u32 = 21;

/// Which kind of Maven project this is. Capabilities wire themselves up
/// differently in each (a Spring project gets starters and autoconfiguration;
/// a plain one gets the library plus hand-rolled glue), and this is always
/// detected from the pom rather than asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    SpringBoot,
    PlainMaven,
}

/// A dependency to splice in. `version: None` means the version is supplied by
/// dependency management (Spring Boot's parent pom), which is how we avoid
/// baking stale version pins into the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dependency {
    pub group_id: &'static str,
    pub artifact_id: &'static str,
    pub version: Option<&'static str>,
    pub scope: Option<&'static str>,
    /// Development-only modules such as `spring-boot-docker-compose`.
    pub optional: bool,
}

pub fn read(root: &Path) -> Result<String> {
    let path = root.join("pom.xml");
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))
}

pub fn flavor(pom: &str) -> Flavor {
    if pom.contains("spring-boot-starter-parent") || pom.contains("spring-boot-dependencies") {
        Flavor::SpringBoot
    } else {
        Flavor::PlainMaven
    }
}

/// The Java release the project compiles against, from whichever of the three
/// usual spellings it uses. `None` when the pom says nothing (Maven then
/// defaults to something ancient, so callers should treat that as "too old").
pub fn release_level(pom: &str) -> Option<u32> {
    for tag in [
        "maven.compiler.release",
        "java.version",
        "maven.compiler.source",
    ] {
        if let Some(value) = element_text(pom, tag) {
            // `java.version` is sometimes written `1.8`; take the last segment
            // so both `1.8` and `27` land on a sane number.
            let value = value.trim();
            let numeric = value.rsplit('.').next().unwrap_or(value);
            if let Ok(n) = numeric.parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

/// First text content of `<tag>...</tag>`, ignoring commented-out copies.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&open) {
        let start = from + rel;
        if !inside_comment(xml, start) {
            let text_start = start + open.len();
            let end = xml[text_start..].find(&close)? + text_start;
            return Some(xml[text_start..end].to_string());
        }
        from = start + open.len();
    }
    None
}

fn inside_comment(xml: &str, offset: usize) -> bool {
    match xml[..offset].rfind("<!--") {
        Some(open) => xml[open..offset].find("-->").is_none(),
        None => false,
    }
}

pub fn has_dependency(pom: &str, group_id: &str, artifact_id: &str) -> bool {
    // Match on the artifactId and then confirm the groupId appears within the
    // same <dependency> block, so `commons-csv` in one dependency doesn't get
    // confused with a different group's identically named artifact.
    let needle = format!("<artifactId>{artifact_id}</artifactId>");
    let group = format!("<groupId>{group_id}</groupId>");
    let mut from = 0;
    while let Some(rel) = pom[from..].find(&needle) {
        let at = from + rel;
        if !inside_comment(pom, at) {
            let block_start = pom[..at].rfind("<dependency>").unwrap_or(0);
            let block_end = pom[at..]
                .find("</dependency>")
                .map(|i| at + i)
                .unwrap_or(pom.len());
            if pom[block_start..block_end].contains(&group) {
                return true;
            }
        }
        from = at + needle.len();
    }
    false
}

/// A tag found by the scanner, with byte offsets into the original string.
#[derive(Debug)]
struct Tag {
    name: String,
    start: usize,
    closing: bool,
    self_closing: bool,
}

/// Scan XML into a flat tag list, skipping comments, CDATA, the XML
/// declaration and doctypes. Deliberately minimal -- it only needs to be
/// right about element nesting, not about attributes or entities.
fn scan_tags(xml: &str) -> Vec<Tag> {
    let bytes = xml.as_bytes();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &xml[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i += rest.find("]]>").map(|e| e + 3).unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i += rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let inner = &rest[1..gt];
        let closing = inner.starts_with('/');
        let self_closing = inner.ends_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        if !name.is_empty() {
            tags.push(Tag {
                name,
                start: i,
                closing,
                self_closing,
            });
        }
        i += gt + 1;
    }
    tags
}

/// Byte offset of the `</dependencies>` that closes the *project-level*
/// `<dependencies>` element -- not one nested inside `<dependencyManagement>`,
/// a `<plugin>`, or a `<profile>`.
fn project_dependencies_close(xml: &str) -> Option<usize> {
    let tags = scan_tags(xml);
    let mut stack: Vec<&str> = Vec::new();
    let mut depth_of_target: Option<usize> = None;

    for tag in &tags {
        if tag.closing {
            if let Some(target_depth) = depth_of_target {
                if stack.len() == target_depth && tag.name == "dependencies" {
                    return Some(tag.start);
                }
            }
            stack.pop();
            continue;
        }
        if tag.self_closing {
            continue;
        }
        if tag.name == "dependencies"
            && stack.as_slice() == ["project"]
            && depth_of_target.is_none()
        {
            depth_of_target = Some(stack.len() + 1);
        }
        stack.push(&tag.name);
    }
    None
}

/// Indentation of the line `offset` sits on, when that line starts with
/// whitespace only.
fn line_indent(xml: &str, offset: usize) -> Option<&str> {
    let line_start = xml[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let prefix = &xml[line_start..offset];
    prefix
        .chars()
        .all(|c| c == ' ' || c == '\t')
        .then_some(prefix)
}

fn render_dependency(dep: &Dependency, indent: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{indent}<dependency>\n"));
    out.push_str(&format!(
        "{indent}    <groupId>{}</groupId>\n",
        dep.group_id
    ));
    out.push_str(&format!(
        "{indent}    <artifactId>{}</artifactId>\n",
        dep.artifact_id
    ));
    if let Some(v) = dep.version {
        out.push_str(&format!("{indent}    <version>{v}</version>\n"));
    }
    if let Some(s) = dep.scope {
        out.push_str(&format!("{indent}    <scope>{s}</scope>\n"));
    }
    if dep.optional {
        out.push_str(&format!("{indent}    <optional>true</optional>\n"));
    }
    out.push_str(&format!("{indent}</dependency>\n"));
    out
}

/// Splice `dep` into `pom`, returning the new text. Returns `Ok(None)` when
/// the dependency is already declared -- `add` is idempotent, so re-running it
/// reports "already present" instead of writing a duplicate.
pub fn add_dependency(pom: &str, dep: &Dependency) -> Result<Option<String>> {
    if has_dependency(pom, dep.group_id, dep.artifact_id) {
        return Ok(None);
    }

    if let Some(close) = project_dependencies_close(pom) {
        // Preferred case: insert a whole line above `</dependencies>`, using
        // the indentation of the last existing <dependency> when there is one.
        let child_indent = last_child_indent(pom, close).unwrap_or_else(|| {
            let base = line_indent(pom, close).unwrap_or("    ");
            format!("{base}    ")
        });
        if let Some(_close_indent) = line_indent(pom, close) {
            let line_start = pom[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let mut out = String::with_capacity(pom.len() + 256);
            out.push_str(&pom[..line_start]);
            out.push_str(&render_dependency(dep, &child_indent));
            out.push_str(&pom[line_start..]);
            return Ok(Some(out));
        }
        // `<dependencies></dependencies>` on one line: break it open.
        let mut out = String::with_capacity(pom.len() + 256);
        out.push_str(&pom[..close]);
        out.push('\n');
        out.push_str(&render_dependency(dep, &child_indent));
        out.push_str(&pom[close..]);
        return Ok(Some(out));
    }

    // No project-level <dependencies> at all: create one before </project>.
    let close_project = pom
        .rfind("</project>")
        .ok_or_else(|| "pom.xml has no </project> -- is it a valid Maven pom?".to_string())?;
    let line_start = pom[..close_project].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent = line_indent(pom, close_project).unwrap_or("");
    let child = format!("{indent}    ");
    let mut block = String::new();
    block.push_str(&format!("{indent}    <dependencies>\n"));
    block.push_str(&render_dependency(dep, &format!("{child}    ")));
    block.push_str(&format!("{indent}    </dependencies>\n"));

    let mut out = String::with_capacity(pom.len() + 256);
    out.push_str(&pom[..line_start]);
    out.push_str(&block);
    out.push_str(&pom[line_start..]);
    Ok(Some(out))
}

/// Indentation of the last `<dependency>` opening tag before `close`, so a
/// spliced dependency lines up with its siblings whatever the file's style is.
fn last_child_indent(pom: &str, close: usize) -> Option<String> {
    let at = pom[..close].rfind("<dependency>")?;
    line_indent(pom, at).map(|s| s.to_string())
}

/// Remove a project-level `<dependency>` matching `group_id`/`artifact_id`.
/// Returns `Ok(None)` when it is not declared -- `remove` is idempotent the
/// same way `add` is. Skips `<dependencyManagement>` so a BOM entry is never
/// mistaken for the real declaration.
pub fn remove_dependency(pom: &str, group_id: &str, artifact_id: &str) -> Result<Option<String>> {
    let Some((start, end)) = project_child_span(
        pom,
        "dependency",
        &["project", "dependencies"],
        group_id,
        artifact_id,
    ) else {
        return Ok(None);
    };
    Ok(Some(cut(pom, start, end)))
}

/// Remove a `project/build/plugins` `<plugin>` whose artifactId matches.
pub fn remove_plugin(pom: &str, artifact_id: &str) -> Result<Option<String>> {
    let Some((start, end)) = plugin_span(pom, artifact_id) else {
        return Ok(None);
    };
    Ok(Some(cut(pom, start, end)))
}

fn cut(xml: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(xml.len() - (end - start));
    out.push_str(&xml[..start]);
    out.push_str(&xml[end..]);
    out
}

fn span_including_line(xml: &str, start: usize, end: usize) -> (usize, usize) {
    let line_start = xml[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let start = if xml[line_start..start]
        .chars()
        .all(|c| c == ' ' || c == '\t')
    {
        line_start
    } else {
        start
    };
    let end = if xml[end..].starts_with('\n') {
        end + 1
    } else if xml[end..].starts_with("\r\n") {
        end + 2
    } else {
        end
    };
    (start, end)
}

/// Byte span of a project-level `<dependency>` whose groupId and artifactId
/// match. `parent_path` is the element stack that must surround the opening
/// tag (`["project", "dependencies"]`).
fn project_child_span(
    xml: &str,
    element: &str,
    parent_path: &[&str],
    group_id: &str,
    artifact_id: &str,
) -> Option<(usize, usize)> {
    let tags = scan_tags(xml);
    let mut stack: Vec<&str> = Vec::new();
    let mut open_at: Option<usize> = None;
    let group = format!("<groupId>{group_id}</groupId>");
    let artifact = format!("<artifactId>{artifact_id}</artifactId>");

    for tag in &tags {
        if tag.closing {
            if tag.name == element
                && let Some(start) = open_at
            {
                let close_end = tag.start + format!("</{element}>").len();
                let block = &xml[start..close_end.min(xml.len())];
                if block.contains(&group) && block.contains(&artifact) {
                    return Some(span_including_line(xml, start, close_end));
                }
                open_at = None;
            }
            stack.pop();
            continue;
        }
        if tag.self_closing {
            continue;
        }
        if tag.name == element && stack.as_slice() == parent_path {
            open_at = Some(tag.start);
        }
        stack.push(&tag.name);
    }
    None
}

fn plugin_span(xml: &str, artifact_id: &str) -> Option<(usize, usize)> {
    let tags = scan_tags(xml);
    let mut stack: Vec<&str> = Vec::new();
    let mut open_at: Option<usize> = None;
    let artifact = format!("<artifactId>{artifact_id}</artifactId>");
    let parent: &[&str] = &["project", "build", "plugins"];

    for tag in &tags {
        if tag.closing {
            if tag.name == "plugin"
                && let Some(start) = open_at
            {
                let close_end = tag.start + "</plugin>".len();
                let block = &xml[start..close_end.min(xml.len())];
                if block.contains(&artifact) {
                    return Some(span_including_line(xml, start, close_end));
                }
                open_at = None;
            }
            stack.pop();
            continue;
        }
        if tag.self_closing {
            continue;
        }
        if tag.name == "plugin" && stack.as_slice() == parent {
            open_at = Some(tag.start);
        }
        stack.push(&tag.name);
    }
    None
}

// ---------------------------------------------------------------------------
// build plugins
// ---------------------------------------------------------------------------

/// Byte offset of the `</plugins>` closing the `project/build/plugins`
/// element. `pluginManagement`, `profiles` and `reporting` all nest an
/// identically named element, so this walks the stack rather than matching
/// text -- same reasoning as `project_dependencies_close`.
fn build_plugins_close(xml: &str) -> Option<usize> {
    let tags = scan_tags(xml);
    let mut stack: Vec<&str> = Vec::new();
    let mut depth_of_target: Option<usize> = None;

    for tag in &tags {
        if tag.closing {
            if let Some(target_depth) = depth_of_target {
                if stack.len() == target_depth && tag.name == "plugins" {
                    return Some(tag.start);
                }
            }
            stack.pop();
            continue;
        }
        if tag.self_closing {
            continue;
        }
        if tag.name == "plugins"
            && stack.as_slice() == ["project", "build"]
            && depth_of_target.is_none()
        {
            depth_of_target = Some(stack.len() + 1);
        }
        stack.push(&tag.name);
    }
    None
}

/// True when `artifact_id` is already declared as a build plugin.
pub fn has_plugin(pom: &str, artifact_id: &str) -> bool {
    let needle = format!("<artifactId>{artifact_id}</artifactId>");
    let mut from = 0;
    while let Some(rel) = pom[from..].find(&needle) {
        let at = from + rel;
        if !inside_comment(pom, at) {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// Splice a whole `<plugin>` block (rendered by the caller, since plugin
/// configuration is far too varied to model as a struct) into
/// `project/build/plugins`, creating `<build>`/`<plugins>` if absent.
/// `Ok(None)` when the plugin is already declared.
pub fn add_plugin(pom: &str, artifact_id: &str, body: &str) -> Result<Option<String>> {
    if has_plugin(pom, artifact_id) {
        return Ok(None);
    }

    if let Some(close) = build_plugins_close(pom) {
        let child_indent = pom[..close]
            .rfind("<plugin>")
            .and_then(|at| line_indent(pom, at))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}    ", line_indent(pom, close).unwrap_or("        ")));
        let line_start = pom[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let mut out = String::with_capacity(pom.len() + body.len() + 128);
        out.push_str(&pom[..line_start]);
        out.push_str(&indent_block(body, &child_indent));
        out.push_str(&pom[line_start..]);
        return Ok(Some(out));
    }

    // No <build><plugins> yet: create the whole nest before </project>.
    let close_project = pom
        .rfind("</project>")
        .ok_or_else(|| "pom.xml has no </project> -- is it a valid Maven pom?".to_string())?;
    let line_start = pom[..close_project].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent = line_indent(pom, close_project).unwrap_or("").to_string();
    let step = format!("{indent}    ");

    let mut block = String::new();
    block.push_str(&format!("{step}<build>\n"));
    block.push_str(&format!("{step}    <plugins>\n"));
    block.push_str(&indent_block(body, &format!("{step}        ")));
    block.push_str(&format!("{step}    </plugins>\n"));
    block.push_str(&format!("{step}</build>\n"));

    let mut out = String::with_capacity(pom.len() + block.len());
    out.push_str(&pom[..line_start]);
    out.push_str(&block);
    out.push_str(&pom[line_start..]);
    Ok(Some(out))
}

/// Re-indent a multi-line block so its first line sits at `indent` and the
/// rest keep their relative shape.
fn indent_block(body: &str, indent: &str) -> String {
    let mut out = String::with_capacity(body.len() + 64);
    for line in body.trim_end().lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else {
            out.push_str(indent);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPRING_POM: &str = r#"<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <parent>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-starter-parent</artifactId>
        <version>4.0.0</version>
    </parent>
    <properties>
        <java.version>27</java.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-web</artifactId>
        </dependency>
    </dependencies>
</project>
"#;

    const PLAIN_POM: &str = r#"<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>demo</artifactId>
    <version>1.0</version>
    <properties>
        <maven.compiler.release>27</maven.compiler.release>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>5.11.4</version>
            <scope>test</scope>
        </dependency>
    </dependencies>
</project>
"#;

    const CSV: Dependency = Dependency {
        group_id: "org.apache.commons",
        artifact_id: "commons-csv",
        version: Some("1.12.0"),
        scope: None,
        optional: false,
    };

    #[test]
    fn flavor_detects_spring_boot_parent() {
        assert_eq!(flavor(SPRING_POM), Flavor::SpringBoot);
        assert_eq!(flavor(PLAIN_POM), Flavor::PlainMaven);
    }

    #[test]
    fn release_level_reads_all_three_spellings() {
        assert_eq!(release_level(SPRING_POM), Some(27));
        assert_eq!(release_level(PLAIN_POM), Some(27));
        assert_eq!(
            release_level(
                "<project><properties><java.version>1.8</java.version></properties></project>"
            ),
            Some(8)
        );
        assert_eq!(release_level("<project/>"), None);
    }

    #[test]
    fn has_dependency_matches_group_and_artifact_together() {
        assert!(has_dependency(
            PLAIN_POM,
            "org.junit.jupiter",
            "junit-jupiter"
        ));
        assert!(!has_dependency(
            PLAIN_POM,
            "org.apache.commons",
            "commons-csv"
        ));
        // Same artifactId, different group -- must not count as present.
        assert!(!has_dependency(PLAIN_POM, "com.example", "junit-jupiter"));
    }

    #[test]
    fn has_dependency_ignores_commented_out_declarations() {
        let pom = "<project><dependencies>\n<!--\n<dependency><groupId>org.apache.commons</groupId><artifactId>commons-csv</artifactId></dependency>\n-->\n</dependencies></project>";
        assert!(!has_dependency(pom, "org.apache.commons", "commons-csv"));
    }

    #[test]
    fn add_dependency_is_idempotent() {
        let once = add_dependency(PLAIN_POM, &CSV)
            .unwrap()
            .expect("first add splices");
        assert!(add_dependency(&once, &CSV).unwrap().is_none());
    }

    #[test]
    fn add_dependency_matches_sibling_indentation_and_preserves_the_rest() {
        let out = add_dependency(PLAIN_POM, &CSV).unwrap().unwrap();
        assert!(out.contains("        <dependency>\n            <groupId>org.apache.commons</groupId>\n            <artifactId>commons-csv</artifactId>\n            <version>1.12.0</version>\n        </dependency>\n"));
        // Everything that was there before is still there, byte for byte.
        assert!(out.contains("<artifactId>junit-jupiter</artifactId>"));
        assert!(out.contains("<maven.compiler.release>27</maven.compiler.release>"));
        assert!(out.ends_with("</project>\n"));
    }

    #[test]
    fn add_dependency_preserves_comments() {
        let pom = "<project>\n    <!-- keep me -->\n    <dependencies>\n    </dependencies>\n</project>\n";
        let out = add_dependency(pom, &CSV).unwrap().unwrap();
        assert!(out.contains("<!-- keep me -->"));
    }

    #[test]
    fn add_dependency_skips_the_dependency_management_block() {
        let pom = r#"<project>
    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>com.example</groupId>
                <artifactId>bom</artifactId>
            </dependency>
        </dependencies>
    </dependencyManagement>
    <dependencies>
        <dependency>
            <groupId>com.example</groupId>
            <artifactId>real</artifactId>
        </dependency>
    </dependencies>
</project>
"#;
        let out = add_dependency(pom, &CSV).unwrap().unwrap();
        let managed_end = out.find("</dependencyManagement>").unwrap();
        let spliced = out.find("commons-csv").unwrap();
        assert!(
            spliced > managed_end,
            "dependency landed inside dependencyManagement"
        );
    }

    #[test]
    fn add_dependency_creates_the_element_when_absent() {
        let pom = "<project>\n    <artifactId>demo</artifactId>\n</project>\n";
        let out = add_dependency(pom, &CSV).unwrap().unwrap();
        assert!(out.contains("    <dependencies>\n"));
        assert!(out.contains("<artifactId>commons-csv</artifactId>"));
        assert!(out.contains("    </dependencies>\n</project>"));
    }

    #[test]
    fn add_dependency_omits_version_when_managed_by_a_parent() {
        let managed = Dependency {
            version: None,
            ..CSV
        };
        let out = add_dependency(SPRING_POM, &managed).unwrap().unwrap();
        assert!(out.contains("<artifactId>commons-csv</artifactId>"));
        assert!(!out.contains("<version>1.12.0</version>"));
    }

    #[test]
    fn add_dependency_errors_on_a_pom_without_a_project_element() {
        assert!(add_dependency("nonsense", &CSV).is_err());
    }

    const SPOTLESS: &str =
        "<plugin>\n    <artifactId>spotless-maven-plugin</artifactId>\n</plugin>";

    #[test]
    fn add_plugin_is_idempotent() {
        let pom = "<project>\n    <build>\n        <plugins>\n        </plugins>\n    </build>\n</project>\n";
        let once = add_plugin(pom, "spotless-maven-plugin", SPOTLESS)
            .unwrap()
            .unwrap();
        assert!(
            add_plugin(&once, "spotless-maven-plugin", SPOTLESS)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn add_plugin_creates_the_build_nest_when_absent() {
        let pom = "<project>\n    <artifactId>demo</artifactId>\n</project>\n";
        let out = add_plugin(pom, "spotless-maven-plugin", SPOTLESS)
            .unwrap()
            .unwrap();
        assert!(out.contains("    <build>\n        <plugins>\n"));
        assert!(out.contains("spotless-maven-plugin"));
        assert!(out.contains("        </plugins>\n    </build>\n</project>"));
    }

    /// `pluginManagement` nests an identically named `<plugins>`; landing in it
    /// would declare a version without ever running the plugin.
    #[test]
    fn add_plugin_skips_the_plugin_management_block() {
        let pom = r#"<project>
    <build>
        <pluginManagement>
            <plugins>
                <plugin>
                    <artifactId>managed</artifactId>
                </plugin>
            </plugins>
        </pluginManagement>
        <plugins>
            <plugin>
                <artifactId>real</artifactId>
            </plugin>
        </plugins>
    </build>
</project>
"#;
        let out = add_plugin(pom, "spotless-maven-plugin", SPOTLESS)
            .unwrap()
            .unwrap();
        let managed_end = out.find("</pluginManagement>").unwrap();
        assert!(out.find("spotless-maven-plugin").unwrap() > managed_end);
    }

    #[test]
    fn add_plugin_matches_sibling_indentation() {
        let pom = "<project>\n    <build>\n        <plugins>\n            <plugin>\n                <artifactId>real</artifactId>\n            </plugin>\n        </plugins>\n    </build>\n</project>\n";
        let out = add_plugin(pom, "spotless-maven-plugin", SPOTLESS)
            .unwrap()
            .unwrap();
        assert!(out.contains("            <plugin>\n                <artifactId>spotless-maven-plugin</artifactId>\n            </plugin>\n"));
        assert!(
            out.contains("<artifactId>real</artifactId>"),
            "existing plugins survive"
        );
    }

    #[test]
    fn add_dependency_renders_optional() {
        let optional = Dependency {
            optional: true,
            ..CSV
        };
        let out = add_dependency(PLAIN_POM, &optional).unwrap().unwrap();
        assert!(out.contains("<artifactId>commons-csv</artifactId>"));
        assert!(out.contains("<optional>true</optional>"));
    }

    #[test]
    fn remove_dependency_is_the_inverse_of_add() {
        let added = add_dependency(PLAIN_POM, &CSV).unwrap().unwrap();
        let removed = remove_dependency(&added, CSV.group_id, CSV.artifact_id)
            .unwrap()
            .unwrap();
        assert_eq!(removed, PLAIN_POM);
    }

    #[test]
    fn remove_dependency_is_idempotent() {
        assert!(
            remove_dependency(PLAIN_POM, CSV.group_id, CSV.artifact_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn remove_dependency_skips_the_dependency_management_block() {
        let pom = r#"<project>
    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>org.apache.commons</groupId>
                <artifactId>commons-csv</artifactId>
                <version>1.0</version>
            </dependency>
        </dependencies>
    </dependencyManagement>
    <dependencies>
        <dependency>
            <groupId>com.example</groupId>
            <artifactId>real</artifactId>
        </dependency>
    </dependencies>
</project>
"#;
        assert!(
            remove_dependency(pom, "org.apache.commons", "commons-csv")
                .unwrap()
                .is_none()
        );
        assert!(pom.contains("commons-csv"));
    }

    #[test]
    fn remove_plugin_is_the_inverse_of_add() {
        let pom = "<project>\n    <build>\n        <plugins>\n            <plugin>\n                <artifactId>real</artifactId>\n            </plugin>\n        </plugins>\n    </build>\n</project>\n";
        let added = add_plugin(pom, "spotless-maven-plugin", SPOTLESS)
            .unwrap()
            .unwrap();
        let removed = remove_plugin(&added, "spotless-maven-plugin")
            .unwrap()
            .unwrap();
        assert_eq!(removed, pom);
    }

    #[test]
    fn remove_plugin_leaves_sibling_plugins() {
        let pom = "<project>\n    <build>\n        <plugins>\n            <plugin>\n                <artifactId>real</artifactId>\n            </plugin>\n            <plugin>\n                <artifactId>spotless-maven-plugin</artifactId>\n            </plugin>\n        </plugins>\n    </build>\n</project>\n";
        let out = remove_plugin(pom, "spotless-maven-plugin")
            .unwrap()
            .unwrap();
        assert!(out.contains("<artifactId>real</artifactId>"));
        assert!(!out.contains("spotless-maven-plugin"));
    }
}
