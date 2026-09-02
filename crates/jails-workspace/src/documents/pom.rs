//! The one reader of `pom.xml`, and the one thing that splices into it.
//!
//! **Everything jails knows about Maven's XML is here.** Capture asks it what
//! this project declares, the dependency and build-feature adapters beside it
//! ask where a block goes, and `new` and `modernize` ask it for the two edits
//! they make before a model exists. A second scanner is how `doctor` comes to
//! name a dependency the build does not have: two readers of one format agree
//! until the day a pom is written in a shape only one of them expected, and
//! nothing says which one is wrong.
//!
//! **It is recognition, not understanding.** jails never resolves a build.
//! An unreadable pom yields nothing rather than a guess, and every caller
//! reads "not stated here" as *unknown* rather than as *absent*. The scanner
//! only has to be right about element nesting -- not about attributes,
//! entities or namespaces -- which is what keeps it from growing into a
//! parser.
//!
//! Edits are targeted splices at a byte offset, never an XML round-trip: a
//! real XML crate parses more correctly and reformats the whole document on
//! write, which is unacceptable for a file people maintain by hand.

use super::{indent_block, insert_indented_block};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// the walk
// ---------------------------------------------------------------------------

/// A tag found by the scanner, with byte offsets into the original string.
#[derive(Debug)]
struct Tag {
    name: String,
    start: usize,
    closing: bool,
    self_closing: bool,
}

/// Scan XML into a flat tag list, skipping comments, CDATA, the XML
/// declaration and doctypes.
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

/// Byte offset of the tag closing the element at exactly this path.
///
/// `["project", "build", "plugins"]` finds the `</plugins>` that closes
/// `project/build/plugins` and not the identically named one nested in
/// `<pluginManagement>`, `<profiles>` or `<reporting>`, which is why this
/// walks the stack rather than matching text.
pub(crate) fn direct_child_close(xml: &str, target: &[&str]) -> Option<usize> {
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

/// Whether `offset` sits inside an XML comment.
fn inside_comment(xml: &str, offset: usize) -> bool {
    match xml[..offset].rfind("<!--") {
        Some(open) => xml[open..offset].find("-->").is_none(),
        None => false,
    }
}

/// The text between the first `start` and the next `end`.
fn between<'a>(source: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let source = source.split_once(start)?.1;
    source.split_once(end).map(|(value, _)| value)
}

/// First text content of `<tag>...</tag>`, ignoring commented-out copies.
fn element_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&open) {
        let start = from + rel;
        if !inside_comment(xml, start) {
            let text_start = start + open.len();
            let end = xml[text_start..].find(&close)? + text_start;
            return Some(&xml[text_start..end]);
        }
        from = start + open.len();
    }
    None
}

/// Every `<dependency>` element's body, in document order, skipping any that
/// is commented out.
///
/// The one walk of the dependency list: the coordinate set capture records,
/// the versionless dependencies `doctor` reports and the JUnit version
/// `test --fast` needs are three readings of this iterator rather than three
/// scans that could disagree about what a `<dependency>` is.
fn dependency_blocks(pom: &str) -> impl Iterator<Item = &str> {
    let mut from = 0;
    std::iter::from_fn(move || {
        loop {
            let start = from + pom[from..].find("<dependency>")?;
            let body_start = start + "<dependency>".len();
            let end = pom[body_start..]
                .find("</dependency>")
                .map_or(pom.len(), |at| body_start + at);
            from = end;
            if !inside_comment(pom, start) {
                return Some(&pom[body_start..end]);
            }
        }
    })
}

// ---------------------------------------------------------------------------
// what the build states
// ---------------------------------------------------------------------------

/// Whether this pom declares `group_id:artifact_id`.
///
/// The artifactId is the anchor and the groupId has to appear in the same
/// `<dependency>` block, so `commons-csv` in one dependency is not confused
/// with a different group's identically named artifact.
///
/// **A coordinate outside any `<dependency>` counts too**, because the
/// enclosing element falls back to the whole document: that is what makes
/// `has_dependency(pom, "org.springframework.boot",
/// "spring-boot-starter-parent")` answer the question `doctor` is actually
/// asking, which is whether the project inherits Boot at all.
pub fn has_dependency(pom: &str, group_id: &str, artifact_id: &str) -> bool {
    let needle = format!("<artifactId>{artifact_id}</artifactId>");
    let group = format!("<groupId>{group_id}</groupId>");
    let mut from = 0;
    while let Some(rel) = pom[from..].find(&needle) {
        let at = from + rel;
        if !inside_comment(pom, at) {
            let block_start = pom[..at].rfind("<dependency>").unwrap_or(0);
            let block_end = pom[at..]
                .find("</dependency>")
                .map_or(pom.len(), |offset| at + offset);
            if pom[block_start..block_end].contains(&group) {
                return true;
            }
        }
        from = at + needle.len();
    }
    false
}

/// Every `group:artifact` this pom declares, coordinates only.
///
/// No versions, no scopes, no resolution: jails does not understand a build,
/// and reading one artifact name out of it is not understanding one.
pub fn dependency_coordinates(pom: &str) -> BTreeSet<String> {
    dependency_blocks(pom)
        .filter_map(|block| {
            let group = element_text(block, "groupId")?;
            let artifact = element_text(block, "artifactId")?;
            Some(format!("{}:{}", group.trim(), artifact.trim()))
        })
        .collect()
}

/// Whether `artifact_id` is already declared as a build plugin.
pub fn has_plugin(pom: &str, artifact_id: &str) -> bool {
    let needle = format!("<artifactId>{artifact_id}</artifactId>");
    pom.match_indices(&needle)
        .any(|(at, _)| !inside_comment(pom, at))
}

/// The Java release the project compiles against, from whichever of the three
/// usual spellings it uses.
///
/// `None` when the pom says nothing -- Maven then defaults to something
/// ancient, so callers should treat that as "too old" rather than as "fine".
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
            if let Ok(release) = numeric.parse::<u32>() {
                return Some(release);
            }
        }
    }
    None
}

/// The class the packaged jar starts, as the POM declares it.
///
/// Maven's own record of the entry point, which is why it is the one jails
/// reads rather than searching source for a `main`. A project with two
/// dispatchers -- `new-cli` writes `App.java` and `generate cli` writes a
/// second -- has two `main` methods, and a search picks whichever the
/// directory walk reached first. `java -jar` never guesses; neither should
/// anything claiming to run the same application.
pub fn main_class(pom: &str) -> Option<&str> {
    element_text(pom, "mainClass")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The identity the build declares for itself.
///
/// The parent's `<artifactId>` is skipped by dropping the `<parent>` element
/// first: a Boot project's first `<artifactId>` belongs to
/// `spring-boot-starter-parent`, and a consumer group named after it would be
/// the same durable identity in every Boot project on the broker.
pub fn artifact_id(pom: &str) -> Option<String> {
    let outside = match between(pom, "<parent>", "</parent>") {
        Some(parent) => pom.replacen(parent, "", 1),
        None => pom.to_string(),
    };
    element_text(&outside, "artifactId").map(|name| name.trim().to_string())
}

/// The Spring Boot version this pom's `<parent>` pins, when it pins one.
///
/// **The `<parent>` element specifically.** A project inheriting Boot through
/// `spring-boot-dependencies` in `<dependencyManagement>` states its version
/// somewhere this does not look, and the honest answer there is `None` rather
/// than a version read out of an import scope -- which is a different fact
/// from [`is_spring_boot`], and deliberately so.
pub fn parent_spring_boot_version(pom: &str) -> Option<&str> {
    let parent = between(pom, "<parent>", "</parent>")?;
    if !parent.contains("<artifactId>spring-boot-starter-parent</artifactId>") {
        return None;
    }
    between(parent, "<version>", "</version>").map(str::trim)
}

/// The Boot `(major, minor)` this pom's parent declares, when it declares one.
///
/// The major alone is enough to choose an import; it is not enough to choose a
/// *module set*. Boot split `spring-boot-testcontainers` out at 3.1, began
/// managing `flyway-database-postgresql` at 3.3, and only moved Flyway's
/// auto-configuration into `spring-boot-flyway` at 4.0 -- three boundaries
/// inside two majors, and `add db` needs all three. `None` means the parent is
/// absent or unreadable, which is a different answer from "old".
pub fn spring_boot_version_of(pom: &str) -> Option<(u32, u32)> {
    let mut parts = parent_spring_boot_version(pom)?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

/// The Spring Boot major from the parent pom, defaulting to 3 when it cannot
/// be read -- the conservative choice, since the pre-4 package names still
/// exist as deprecated aliases in some builds while the 4 ones simply do not
/// exist before 4.
pub fn spring_boot_major_of(pom: &str) -> u32 {
    spring_boot_version_of(pom).map_or(3, |(major, _)| major)
}

/// Whether this is a Spring Boot project at all.
///
/// Deliberately looser than [`parent_spring_boot_version`]: a project that
/// imports `spring-boot-dependencies` as a BOM is a Boot project whose
/// capabilities wire up the Spring way, even though no `<parent>` states a
/// version. Capabilities ask this; anything that has to *name* a version asks
/// the parent.
pub fn is_spring_boot(pom: &str) -> bool {
    pom.contains("spring-boot-starter-parent") || pom.contains("spring-boot-dependencies")
}

/// The `junit-jupiter` version this pom pins, or `None` when something else
/// manages it.
///
/// Under a Spring Boot parent or an imported `junit-bom` the version is
/// managed and a pin here would be the wrong number to hand the console
/// launcher, so a managed build answers `None` rather than a version.
pub fn junit_jupiter_version(pom: &str) -> Option<&str> {
    if pom.contains("junit-bom") {
        return None;
    }
    dependency_blocks(pom)
        .find(|block| {
            element_text(block, "artifactId").is_some_and(|id| id.trim() == "junit-jupiter")
        })
        .and_then(|block| element_text(block, "version"))
        .map(str::trim)
}

/// Everything about this pom that would stop Maven reading it, in the order a
/// reader should fix them.
///
/// **Structural only, and deliberately so.** `doctor` is read-only by
/// contract, so it cannot run `mvn validate` to find out -- and it should not
/// need to: every one of these is decidable from the text, and the failure
/// they cause is total. A pom missing `modelVersion` fails *every* goal, and a
/// `doctor` that reads an unreadable pom as empty cheerfully reports fifteen
/// checks over a project Maven cannot open at all.
///
/// Each entry is (what is wrong, what to do about it).
pub fn problems(pom: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    if !pom.contains("<project") {
        found.push((
            "pom.xml has no <project> element".to_string(),
            "this is not a Maven pom -- `jails new <name>` writes one".to_string(),
        ));
        return found;
    }
    if element_text(pom, "modelVersion").is_none() {
        found.push((
            "no <modelVersion> -- Maven refuses the pom outright, so every goal fails".to_string(),
            "add <modelVersion>4.0.0</modelVersion> as the first child of <project>".to_string(),
        ));
    }
    let has_parent = pom.contains("<parent>");
    if element_text(pom, "artifactId").is_none() {
        found.push((
            "no <artifactId>".to_string(),
            "give the project an artifactId".to_string(),
        ));
    }
    // groupId and version may be inherited, so they are only missing when
    // there is no parent to inherit them from.
    if !has_parent {
        for (tag, what) in [("groupId", "groupId"), ("version", "version")] {
            if element_text(pom, tag).is_none() {
                found.push((
                    format!("no <{tag}> and no <parent> to inherit it from"),
                    format!("declare the project's {what}"),
                ));
            }
        }
    }
    if !manages_versions(pom) {
        for (group, artifact) in versionless_dependencies(pom) {
            found.push((
                format!(
                    "dependency {group}:{artifact} has no <version>, and this project has no parent or dependencyManagement to supply one"
                ),
                "pin the version, or add the BOM that manages it".to_string(),
            ));
        }
    }
    found
}

/// Does anything in this pom supply managed versions?
fn manages_versions(pom: &str) -> bool {
    pom.contains("<parent>") || pom.contains("<dependencyManagement>")
}

/// Every `<dependency>` with no `<version>`, as (groupId, artifactId).
fn versionless_dependencies(pom: &str) -> Vec<(String, String)> {
    dependency_blocks(pom)
        .filter(|block| element_text(block, "version").is_none())
        .map(|block| {
            (
                element_text(block, "groupId")
                    .unwrap_or_default()
                    .to_string(),
                element_text(block, "artifactId")
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// the two edits made before a plan exists
// ---------------------------------------------------------------------------

/// Splice a whole `<plugin>` block into `project/build/plugins`, creating the
/// containers it needs. `Ok(None)` when the plugin is already declared, so a
/// re-run reports "already present" rather than writing a duplicate.
///
/// The block is rendered by the caller, because plugin configuration is far
/// too varied to model as a struct.
pub fn add_plugin(
    pom: &str,
    artifact_id: &str,
    body: &str,
) -> std::result::Result<Option<String>, String> {
    if has_plugin(pom, artifact_id) {
        return Ok(None);
    }
    insert_plugin(pom, &plugin_nest(body)).map(Some)
}

/// The three shapes a plugin block takes, by how much of
/// `project/build/plugins` the pom already has: the block alone, the block
/// inside a `<plugins>` this creates, and the block inside a whole
/// `<build><plugins>` nest.
///
/// **Written once and read twice.** [`insert_plugin`] inserts whichever one
/// fits, and a caller that owns a marked block compares the same three against
/// what it finds on disk to tell a reader's edit from its own writing. Two
/// lists of these shapes drift on exactly the case nobody has a pom for.
pub fn plugin_nest(block: &str) -> [String; 3] {
    [
        block.to_string(),
        format!("<plugins>\n{}</plugins>\n", indent_block(block, "    ")),
        format!(
            "<build>\n    <plugins>\n{}    </plugins>\n</build>\n",
            indent_block(block, "        ")
        ),
    ]
}

/// Insert a plugin at `project/build/plugins`, creating the containers the pom
/// is missing.
pub(crate) fn insert_plugin(
    text: &str,
    shapes: &[String; 3],
) -> std::result::Result<String, String> {
    if let Some(at) = direct_child_close(text, &["project", "build", "plugins"]) {
        return Ok(insert_indented_block(text, at, &shapes[0], 0));
    }
    if let Some(at) = direct_child_close(text, &["project", "build"]) {
        return Ok(insert_indented_block(text, at, &shapes[1], 0));
    }
    let Some(at) = direct_child_close(text, &["project"]) else {
        return Err(
            "pom.xml has no closing project element\n       fix: repair the Maven POM, then re-plan"
                .to_string(),
        );
    };
    Ok(insert_indented_block(text, at, &shapes[2], 0))
}

/// The POM with `spring-boot-starter-parent` pinned to `version`.
///
/// `None` when it already is, or when there is no readable parent version --
/// a project inheriting Boot through `spring-boot-dependencies` in
/// `<dependencyManagement>` says its version somewhere this does not look, and
/// rewriting the wrong `<version>` in a POM is the worst edit available.
pub fn with_parent_version(pom: &str, version: &str) -> Option<String> {
    let current = parent_spring_boot_version(pom)?;
    if current == version {
        return None;
    }
    let start = current.as_ptr() as usize - pom.as_ptr() as usize;
    let mut out = pom.to_string();
    out.replace_range(start..start + current.len(), version);
    Some(out)
}

/// The POM targeting `release`, through whichever property already says so.
///
/// Only a property that is already there is rewritten. Adding
/// `<maven.compiler.release>` to a POM that inherits its release from a parent
/// would be jails deciding something the project deliberately left to the
/// parent -- and `MIN_RELEASE` exists because an adopted project's release is
/// its own business.
pub fn with_release_level(pom: &str, release: u32) -> Option<String> {
    for tag in [
        "maven.compiler.release",
        "java.version",
        "maven.compiler.source",
        "maven.compiler.target",
    ] {
        let Some(value) = element_text(pom, tag) else {
            continue;
        };
        if value.trim() == release.to_string() {
            continue;
        }
        let start = value.as_ptr() as usize - pom.as_ptr() as usize;
        let mut out = pom.to_string();
        out.replace_range(start..start + value.len(), &release.to_string());
        return Some(out);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT: &str = "<project>\n  <parent>\n    \
                        <groupId>org.springframework.boot</groupId>\n    \
                        <artifactId>spring-boot-starter-parent</artifactId>\n    \
                        <version>2.7.18</version>\n  </parent>\n  <artifactId>demo</artifactId>\n  \
                        <properties>\n    <java.version>21</java.version>\n  </properties>\n\
                        </project>\n";

    /// The two version facts a Maven Spring project carries, moved together.
    #[test]
    fn the_parent_and_the_release_move_and_nothing_else_does() {
        let out = with_parent_version(BOOT, "4.1.0").expect("2.7.18 is not the target");
        assert!(out.contains("<version>4.1.0</version>"), "{out}");
        assert_eq!(with_parent_version(&out, "4.1.0"), None);

        let out = with_release_level(&out, 26).expect("21 is not 26");
        assert!(out.contains("<java.version>26</java.version>"), "{out}");
        assert_eq!(with_release_level(&out, 26), None);
        // The parent's own `<version>` is not a release level, and a scan that
        // took the first `<version>` it saw would have rewritten it.
        assert!(out.contains("<version>4.1.0</version>"), "{out}");
    }

    /// A POM that states no release of its own inherits one, and jails does
    /// not decide for it.
    #[test]
    fn a_release_nothing_states_is_left_to_the_parent() {
        let pom = "<project>\n  <parent>\n    \
                   <artifactId>spring-boot-starter-parent</artifactId>\n    \
                   <version>4.1.0</version>\n  </parent>\n</project>\n";
        assert_eq!(with_release_level(pom, 26), None);
    }

    /// The parent's identity is not the project's, and `artifact_id` is asked
    /// for a durable name a consumer group is built from.
    #[test]
    fn the_projects_own_artifact_id_is_not_its_parents() {
        assert_eq!(artifact_id(BOOT).as_deref(), Some("demo"));
        assert_eq!(spring_boot_version_of(BOOT), Some((2, 7)));
        assert_eq!(spring_boot_major_of(BOOT), 2);
        assert!(is_spring_boot(BOOT));
        assert_eq!(release_level(BOOT), Some(21));
    }

    /// A pom with no Boot parent is read as Boot 3 rather than as unknown,
    /// because a package name has to be chosen either way.
    #[test]
    fn a_pom_with_no_boot_parent_answers_the_conservative_major() {
        assert_eq!(spring_boot_major_of("<project></project>"), 3);
        assert_eq!(spring_boot_version_of("<project></project>"), None);
        assert!(!is_spring_boot("<project></project>"));
    }

    /// A commented-out dependency is not a dependency.
    #[test]
    fn the_dependency_walk_skips_what_is_commented_out() {
        let pom = "<project><dependencies>\n\
                   <!-- <dependency><groupId>dead</groupId>\
                   <artifactId>gone</artifactId></dependency> -->\n\
                   <dependency><groupId>org.jspecify</groupId>\
                   <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\n\
                   </dependencies></project>";
        assert_eq!(
            dependency_coordinates(pom),
            BTreeSet::from(["org.jspecify:jspecify".to_string()])
        );
        assert!(has_dependency(pom, "org.jspecify", "jspecify"));
        assert!(!has_dependency(pom, "dead", "gone"));
    }

    /// A version the project pins is the console launcher's number; a build
    /// that imports the BOM manages it and must not be pinned twice.
    #[test]
    fn the_junit_version_is_only_read_when_nothing_manages_it() {
        let pinned = "<project><dependencies><dependency>\
                      <groupId>org.junit.jupiter</groupId>\
                      <artifactId>junit-jupiter</artifactId>\
                      <version>5.11.4</version></dependency></dependencies></project>";
        assert_eq!(junit_jupiter_version(pinned), Some("5.11.4"));
        let managed = pinned.replace("<project>", "<project><!-- junit-bom -->");
        assert_eq!(junit_jupiter_version(&managed), None);
    }

    /// The plugin nest is created when it is missing, and only once.
    #[test]
    fn a_plugin_lands_inside_build_plugins_however_much_of_it_exists() {
        let bare = "<project>\n    <artifactId>demo</artifactId>\n</project>\n";
        let plugin = "<plugin>\n    <artifactId>maven-enforcer-plugin</artifactId>\n</plugin>\n";
        let once = add_plugin(bare, "maven-enforcer-plugin", plugin)
            .unwrap()
            .expect("the plugin is not declared yet");
        assert!(once.contains("<build>"), "{once}");
        assert!(once.contains("<plugins>"), "{once}");
        assert!(
            add_plugin(&once, "maven-enforcer-plugin", plugin)
                .unwrap()
                .is_none()
        );
    }
}
