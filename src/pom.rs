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

/// The Java release every generated project targets. Referenced by `new-cli`'s
/// pom template, `new`'s `--java` default, and `add`'s precondition check --
/// bumping the target is a one-line change here.
pub const TARGET_RELEASE: &str = "27";

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
    for tag in ["maven.compiler.release", "java.version", "maven.compiler.source"] {
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
            let block_end = pom[at..].find("</dependency>").map(|i| at + i).unwrap_or(pom.len());
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
            tags.push(Tag { name, start: i, closing, self_closing });
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
        if tag.name == "dependencies" && stack.as_slice() == ["project"] && depth_of_target.is_none() {
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
    prefix.chars().all(|c| c == ' ' || c == '\t').then_some(prefix)
}

fn render_dependency(dep: &Dependency, indent: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{indent}<dependency>\n"));
    out.push_str(&format!("{indent}    <groupId>{}</groupId>\n", dep.group_id));
    out.push_str(&format!("{indent}    <artifactId>{}</artifactId>\n", dep.artifact_id));
    if let Some(v) = dep.version {
        out.push_str(&format!("{indent}    <version>{v}</version>\n"));
    }
    if let Some(s) = dep.scope {
        out.push_str(&format!("{indent}    <scope>{s}</scope>\n"));
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
        assert_eq!(release_level("<project><properties><java.version>1.8</java.version></properties></project>"), Some(8));
        assert_eq!(release_level("<project/>"), None);
    }

    #[test]
    fn has_dependency_matches_group_and_artifact_together() {
        assert!(has_dependency(PLAIN_POM, "org.junit.jupiter", "junit-jupiter"));
        assert!(!has_dependency(PLAIN_POM, "org.apache.commons", "commons-csv"));
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
        let once = add_dependency(PLAIN_POM, &CSV).unwrap().expect("first add splices");
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
        assert!(spliced > managed_end, "dependency landed inside dependencyManagement");
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
        let managed = Dependency { version: None, ..CSV };
        let out = add_dependency(SPRING_POM, &managed).unwrap().unwrap();
        assert!(out.contains("<artifactId>commons-csv</artifactId>"));
        assert!(!out.contains("<version>1.12.0</version>"));
    }

    #[test]
    fn add_dependency_errors_on_a_pom_without_a_project_element() {
        assert!(add_dependency("nonsense", &CSV).is_err());
    }
}
