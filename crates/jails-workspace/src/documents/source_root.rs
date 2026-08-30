//! Telling a build tool where the generated tree is.
//!
//! Its own module because it is its own secret: the two build systems answer
//! the same question in shapes that do not resemble each other. Maven needs
//! one `build-helper-maven-plugin` declaration whose executions cover every
//! root -- a declaration per root is a duplicate the model builder warns
//! about -- while Gradle wants a separate source-set block per root, which is
//! how a Gradle reader expects to meet them. `jdl-sol.md` §9.7 names both
//! shapes, and neither one generalises to the other.

use super::{
    direct_child_close, indent_block, insert_at_line, insert_indented_block, line_indent,
    owned_block,
};

/// The legacy per-source-set marker, kept only so its blocks can be absorbed.
const MARKER: &str = "jails:generated-source-root";
/// The one block holding every generated root.
const ROOTS_MARKER: &str = "jails:generated-source-roots";
/// A source-root block is jails' to rewrite only if every path it declares
/// lives here.
const MANAGED_SOURCE_PREFIX: &str = ".jails/generated/";

/// Every generated source root, as one `build-helper-maven-plugin`.
///
/// One marked block holding one `<plugin>` with an `<execution>` per root.
/// It used to be a block per source set, each with its own complete plugin
/// declaration, so a project with a main and a test root declared
/// `org.codehaus.mojo:build-helper-maven-plugin` twice inside one `<plugins>`.
/// Maven merges the executions -- checked against a real build, both roots
/// compile -- but warns `'build.plugins.plugin.(groupId:artifactId)' must be
/// unique but found duplicate declaration` on every single build. Nothing was
/// broken; a permanent alarming warning is its own cost.
///
/// Blocks written by the older shape are absorbed rather than left beside the
/// new one, which would have made three declarations out of two. A block is
/// only jails' to remove if every path it declares is under the managed root:
/// anything else means a reader put it there, and the refusal below says so
/// instead of discarding it.
pub(crate) fn ensure_maven_source_roots(
    text: &str,
    roots: &[jails_contracts::MavenSourceRoot],
) -> Result<String, String> {
    let (mut text, was_at) = strip_source_root_blocks(text)?;
    if roots.is_empty() {
        return Ok(text);
    }

    let open = format!("<!-- {ROOTS_MARKER} -->");
    let close = format!("<!-- /{ROOTS_MARKER} -->");
    let mut plugin = String::new();
    plugin.push_str(&open);
    plugin.push_str("\n<plugin>\n");
    plugin.push_str("    <groupId>org.codehaus.mojo</groupId>\n");
    plugin.push_str("    <artifactId>build-helper-maven-plugin</artifactId>\n");
    plugin.push_str("    <version>3.6.1</version>\n");
    plugin.push_str("    <executions>\n");
    for root in roots {
        let label = source_set_label(root.source_set);
        let source = root.path.as_str();
        let (phase, goal, configuration) = match root.source_set {
            jails_contracts::JavaSourceSet::Main => (
                "generate-sources",
                "add-source",
                format!("<sources><source>{source}</source></sources>"),
            ),
            jails_contracts::JavaSourceSet::Test => (
                "generate-test-sources",
                "add-test-source",
                format!("<sources><source>{source}</source></sources>"),
            ),
            jails_contracts::JavaSourceSet::MainResources => (
                "generate-resources",
                "add-resource",
                format!(
                    "<resources><resource><directory>{source}</directory></resource></resources>"
                ),
            ),
            jails_contracts::JavaSourceSet::TestResources => (
                "generate-test-resources",
                "add-test-resource",
                format!(
                    "<resources><resource><directory>{source}</directory></resource></resources>"
                ),
            ),
        };
        plugin.push_str("        <execution>\n");
        plugin.push_str(&format!(
            "            <id>jails-generated-{label}-source-root</id>\n"
        ));
        plugin.push_str(&format!("            <phase>{phase}</phase>\n"));
        plugin.push_str(&format!("            <goals><goal>{goal}</goal></goals>\n"));
        plugin.push_str("            <configuration>\n");
        plugin.push_str(&format!("                {configuration}\n"));
        plugin.push_str("            </configuration>\n");
        plugin.push_str("        </execution>\n");
    }
    plugin.push_str("    </executions>\n</plugin>\n");
    plugin.push_str(&close);
    plugin.push('\n');

    // **Back where it was, when it was already there.** This block is rebuilt
    // from the model on every run, and reinserting it before `</plugins>`
    // moved it to the end whenever another marked block had been appended
    // after it -- so `add coverage` or `add format` left the next `sync`
    // rewriting `pom.xml` purely to reorder two comments. An adapter whose
    // whole contract is "preserve every other byte" cannot also be the reason
    // a reader-owned file churns.
    if let Some(at) = was_at {
        let indent = direct_child_close(&text, &["project", "build", "plugins"])
            .and_then(|close| line_indent(&text, close))
            .map_or_else(|| "            ".to_string(), |parent| format!("{parent}    "));
        return Ok(insert_at_line(&text, at, &indent_block(&plugin, &indent)));
    }
    if let Some(at) = direct_child_close(&text, &["project", "build", "plugins"]) {
        return Ok(insert_indented_block(&text, at, &plugin, 0));
    }
    if let Some(at) = direct_child_close(&text, &["project", "build"]) {
        let indent = line_indent(&text, at).unwrap_or("    ").to_string();
        let child = format!("{indent}    ");
        let plugins = format!(
            "{child}<plugins>\n{}{child}</plugins>\n",
            indent_block(&plugin, &format!("{child}    "))
        );
        return Ok(insert_at_line(&text, at, &plugins));
    }
    let Some(at) = direct_child_close(&text, &["project"]) else {
        return Err(
            "pom.xml has no closing project element\n       fix: repair the Maven POM, then re-plan"
                .to_string(),
        );
    };
    let indent = line_indent(&text, at).unwrap_or("").to_string();
    let step = format!("{indent}    ");
    let build = format!(
        "{step}<build>\n{step}    <plugins>\n{}{step}    </plugins>\n{step}</build>\n",
        indent_block(&plugin, &format!("{step}        "))
    );
    text = insert_at_line(&text, at, &build);
    Ok(text)
}

/// Remove the source-root blocks jails owns, refusing any a reader has taken
/// over.
///
/// "Owns" is decided by the paths inside: every `<source>`/`<directory>` in
/// the block must be under the managed root. A block naming anything else is
/// not jails' to delete, and saying so beats silently dropping it.
fn strip_source_root_blocks(text: &str) -> Result<(String, Option<usize>), String> {
    let mut text = text.to_string();
    let mut was_at = None;
    // Every legacy per-set block first, then the combined one, so the offset
    // recorded for the combined block is an offset into the text the caller
    // will insert into. `owned_block` matches `<!-- {marker} -->` including
    // the trailing ` -->`, so the singular legacy marker cannot match inside
    // the plural combined one.
    let mut markers = [
        jails_contracts::JavaSourceSet::Main,
        jails_contracts::JavaSourceSet::Test,
        jails_contracts::JavaSourceSet::MainResources,
        jails_contracts::JavaSourceSet::TestResources,
    ]
    .into_iter()
    .map(|source_set| format!("{MARKER}:{}", source_set_label(source_set)))
    .collect::<Vec<_>>();
    markers.push(ROOTS_MARKER.to_string());
    for marker in markers {
        let combined = marker == ROOTS_MARKER;
        let open = format!("<!-- {marker} -->");
        let close = format!("<!-- /{marker} -->");
        while let Some(block) = owned_block(&text, &open, &close)? {
            let block = block.to_string();
            if !declared_paths(&block).all(|path| path.starts_with(MANAGED_SOURCE_PREFIX)) {
                return Err(format!(
                    "the owned Maven generated-source block declares a path jails does not manage\n       fix: remove the complete `{open}` block, or restore its generated roots, then re-plan"
                ));
            }
            let start = text.find(&block).expect("the block was just located");
            let mut end = start + block.len();
            if text[end..].starts_with('\n') {
                end += 1;
            }
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let head = if text[line_start..start].trim().is_empty() {
                line_start
            } else {
                start
            };
            text.replace_range(head..end, "");
            if combined {
                was_at = Some(head);
            }
        }
    }
    Ok((text, was_at))
}

/// The paths a source-root block declares, from either element it can use.
fn declared_paths(block: &str) -> impl Iterator<Item = &str> {
    ["<source>", "<directory>"]
        .into_iter()
        .flat_map(move |tag| {
            let closing = tag.replace('<', "</");
            block.match_indices(tag).filter_map(move |(at, _)| {
                let rest = &block[at + tag.len()..];
                rest.find(closing.as_str()).map(|end| &rest[..end])
            })
        })
}

pub(crate) fn ensure_gradle_source_root(
    text: &str,
    source: &str,
    source_set: jails_contracts::JavaSourceSet,
    kotlin: bool,
) -> Result<String, String> {
    let label = source_set_label(source_set);
    let marker = format!("{MARKER}:{label}");
    let open = format!("// {marker}");
    let close = format!("// /{marker}");
    if let Some(block) = owned_block(text, &open, &close)? {
        if block.contains(source) {
            return Ok(text.to_string());
        }
        return Err(format!(
            "the owned Gradle generated-source block was edited\n       fix: restore `{source}` inside `{open}`, or remove the complete marked block and re-plan"
        ));
    }
    let (set, collection) = match source_set {
        jails_contracts::JavaSourceSet::Main => ("main", "java"),
        jails_contracts::JavaSourceSet::Test => ("test", "java"),
        jails_contracts::JavaSourceSet::MainResources => ("main", "resources"),
        jails_contracts::JavaSourceSet::TestResources => ("test", "resources"),
    };
    let body = if kotlin {
        format!(
            "sourceSets {{\n    named(\"{set}\") {{\n        {collection}.srcDir(\"{source}\")\n    }}\n}}"
        )
    } else {
        format!("sourceSets {{\n    {set} {{\n        {collection}.srcDir('{source}')\n    }}\n}}")
    };
    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(format!("{text}{separator}\n{open}\n{body}\n{close}\n"))
}

fn source_set_label(source_set: jails_contracts::JavaSourceSet) -> &'static str {
    match source_set {
        jails_contracts::JavaSourceSet::Main => "main",
        jails_contracts::JavaSourceSet::Test => "test",
        jails_contracts::JavaSourceSet::MainResources => "main-resources",
        jails_contracts::JavaSourceSet::TestResources => "test-resources",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_contracts::{JavaSourceSet, ProjectPath};

    #[test]
    fn maven_patch_is_lossless_idempotent_and_avoids_plugin_management() {
        let pom = "<project>\n    <build>\n        <pluginManagement><plugins></plugins></pluginManagement>\n        <plugins>\n        </plugins>\n    </build>\n</project>\n";
        let roots = [root(JavaSourceSet::Main, ".jails/generated/main/java")];
        let once = ensure_maven_source_roots(pom, &roots).unwrap();
        assert!(once.contains("build-helper-maven-plugin"));
        assert!(once.find("build-helper").unwrap() > once.find("</pluginManagement>").unwrap());
        assert_eq!(ensure_maven_source_roots(&once, &roots).unwrap(), once);
    }

    #[test]
    fn maven_patch_creates_the_missing_build_nest() {
        let pom = "<project>\n    <modelVersion>4.0.0</modelVersion>\n</project>\n";
        let patched = ensure_maven_source_roots(
            pom,
            &[root(JavaSourceSet::Main, ".jails/generated/main/java")],
        )
        .unwrap();
        assert!(patched.contains("<build>\n        <plugins>"), "{patched}");
        assert!(patched.ends_with("</project>\n"));
    }

    #[test]
    fn gradle_patch_uses_the_script_dialect_and_is_idempotent() {
        let groovy =
            ensure_gradle_source_root("plugins {}\n", "generated/java", JavaSourceSet::Main, false)
                .unwrap();
        assert!(groovy.contains("java.srcDir('generated/java')"));
        assert_eq!(
            ensure_gradle_source_root(&groovy, "generated/java", JavaSourceSet::Main, false,)
                .unwrap(),
            groovy
        );
        let kotlin =
            ensure_gradle_source_root("plugins {}\n", "generated/java", JavaSourceSet::Main, true)
                .unwrap();
        assert!(kotlin.contains("named(\"main\")"));
        assert!(kotlin.contains("java.srcDir(\"generated/java\")"));
    }

    fn root(source_set: JavaSourceSet, path: &str) -> jails_contracts::MavenSourceRoot {
        jails_contracts::MavenSourceRoot {
            source_set,
            path: ProjectPath::parse(path).unwrap(),
        }
    }

    /// Every generated root joins one plugin declaration.
    ///
    /// `audit.md` A2.1b. A block per source set meant a complete
    /// `build-helper-maven-plugin` declaration per set, so main plus test
    /// declared the same plugin twice in one `<plugins>` and Maven warned
    /// `must be unique but found duplicate declaration` on every build. It
    /// merges the executions, so nothing was broken -- which is exactly why it
    /// went unnoticed.
    #[test]
    fn every_generated_root_joins_one_plugin_declaration() {
        let pom = ensure_maven_source_roots(
            "<project>\n</project>\n",
            &[
                root(JavaSourceSet::Main, ".jails/generated/main/java"),
                root(JavaSourceSet::Test, ".jails/generated/test/java"),
                root(
                    JavaSourceSet::TestResources,
                    ".jails/generated/test/resources",
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            pom.matches("build-helper-maven-plugin").count(),
            1,
            "one plugin declaration, whatever the root count:\n{pom}"
        );
        assert_eq!(pom.matches("<execution>").count(), 3, "{pom}");
        assert!(pom.contains("<goal>add-source</goal>"), "{pom}");
        assert!(pom.contains("<goal>add-test-source</goal>"), "{pom}");
        assert!(pom.contains("<goal>add-test-resource</goal>"), "{pom}");
    }

    /// A pom written by the older shape is absorbed, not added to.
    ///
    /// Leaving the per-set blocks beside the new one would have turned two
    /// duplicate declarations into three.
    #[test]
    fn legacy_per_source_set_blocks_are_absorbed() {
        let legacy = concat!(
            "<project>\n  <build>\n    <plugins>\n",
            "      <!-- jails:generated-source-root:main -->\n",
            "      <plugin>\n        <artifactId>build-helper-maven-plugin</artifactId>\n",
            "        <configuration><sources><source>.jails/generated/main/java</source></sources></configuration>\n",
            "      </plugin>\n",
            "      <!-- /jails:generated-source-root:main -->\n",
            "      <!-- jails:generated-source-root:test -->\n",
            "      <plugin>\n        <artifactId>build-helper-maven-plugin</artifactId>\n",
            "        <configuration><sources><source>.jails/generated/test/java</source></sources></configuration>\n",
            "      </plugin>\n",
            "      <!-- /jails:generated-source-root:test -->\n",
            "    </plugins>\n  </build>\n</project>\n"
        );
        let pom = ensure_maven_source_roots(
            legacy,
            &[
                root(JavaSourceSet::Main, ".jails/generated/main/java"),
                root(JavaSourceSet::Test, ".jails/generated/test/java"),
            ],
        )
        .unwrap();
        assert_eq!(
            pom.matches("build-helper-maven-plugin").count(),
            1,
            "the legacy declarations are gone, not joined:\n{pom}"
        );
        assert!(!pom.contains("jails:generated-source-root:main"), "{pom}");
        assert!(!pom.contains("jails:generated-source-root:test"), "{pom}");
        assert!(pom.contains("jails:generated-source-roots"), "{pom}");
    }

    /// A block naming a path jails does not manage is the reader's.
    #[test]
    fn a_reader_owned_source_root_block_refuses_rather_than_vanishing() {
        let reader = concat!(
            "<project>\n  <build>\n    <plugins>\n",
            "      <!-- jails:generated-source-root:main -->\n",
            "      <plugin>\n        <artifactId>build-helper-maven-plugin</artifactId>\n",
            "        <configuration><sources><source>src/extra/java</source></sources></configuration>\n",
            "      </plugin>\n",
            "      <!-- /jails:generated-source-root:main -->\n",
            "    </plugins>\n  </build>\n</project>\n"
        );
        let error = ensure_maven_source_roots(
            reader,
            &[root(JavaSourceSet::Main, ".jails/generated/main/java")],
        )
        .unwrap_err();
        assert!(error.contains("does not manage"), "{error}");
    }

    #[test]
    fn generated_test_resources_use_the_build_tools_resource_set() {
        let pom = ensure_maven_source_roots(
            "<project>\n</project>\n",
            &[root(
                JavaSourceSet::TestResources,
                ".jails/generated/test/resources",
            )],
        )
        .unwrap();
        assert!(pom.contains("<goal>add-test-resource</goal>"), "{pom}");
        assert!(
            pom.contains("<directory>.jails/generated/test/resources</directory>"),
            "{pom}"
        );
        assert!(pom.contains("jails:generated-source-roots"));

        let gradle = ensure_gradle_source_root(
            "plugins {}\n",
            ".jails/generated/test/resources",
            JavaSourceSet::TestResources,
            false,
        )
        .unwrap();
        assert!(
            gradle.contains("resources.srcDir('.jails/generated/test/resources')"),
            "{gradle}"
        );

        let main = ensure_maven_source_roots(
            "<project>\n</project>\n",
            &[root(
                JavaSourceSet::MainResources,
                ".jails/generated/main/resources",
            )],
        )
        .unwrap();
        assert!(main.contains("<goal>add-resource</goal>"), "{main}");
        assert!(
            main.contains("<directory>.jails/generated/main/resources</directory>"),
            "{main}"
        );
    }
}
