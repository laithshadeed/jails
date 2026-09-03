//! Taking the generated-source-root block out of a build file jails wrote
//! before managed output lived under `src/`.
//!
//! Older releases declared `.jails/generated/{main,test}/{java,resources}` to
//! the build: one marked `build-helper-maven-plugin` block on Maven, and a
//! marked source-set block per root on Gradle. Once the files are beside the
//! reader's own sources those declarations point at directories that no
//! longer exist, and Maven would compile nothing from them while Gradle would
//! refuse the missing `srcDir`. `jails model relocate` removes them, and this
//! is the removal: every marker the older shapes ever used, in both comment
//! syntaxes, with the whole line each block sat on.

use super::owned_block;

/// The per-source-set marker, and the combined one that replaced it.
const MARKER: &str = "jails:generated-source-root";
const ROOTS_MARKER: &str = "jails:generated-source-roots";
const LABELS: [&str; 4] = ["main", "test", "main-resources", "test-resources"];

/// The build file with every jails-owned source-root block removed.
///
/// Unchanged text means there was none. A block whose closing marker is
/// missing is an error rather than a guess, as everywhere `owned_block` is
/// read.
pub fn strip_generated_source_roots(text: &str) -> Result<String, jails_model::Diagnostic> {
    let mut text = text.to_string();
    let mut markers = vec![ROOTS_MARKER.to_string()];
    markers.extend(LABELS.iter().map(|label| format!("{MARKER}:{label}")));
    for marker in markers {
        for (open, close) in [
            (format!("<!-- {marker} -->"), format!("<!-- /{marker} -->")),
            (format!("// {marker}"), format!("// /{marker}")),
        ] {
            while let Some(block) = owned_block(&text, &open, &close)? {
                let start = block.as_ptr() as usize - text.as_ptr() as usize;
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
            }
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_combined_maven_block_goes_with_its_lines() {
        let pom = "<project>\n    <build>\n        <plugins>\n            <!-- jails:generated-source-roots -->\n            <plugin>\n                <artifactId>build-helper-maven-plugin</artifactId>\n            </plugin>\n            <!-- /jails:generated-source-roots -->\n            <plugin>\n                <artifactId>maven-failsafe-plugin</artifactId>\n            </plugin>\n        </plugins>\n    </build>\n</project>\n";
        let stripped = strip_generated_source_roots(pom).unwrap();
        assert_eq!(
            stripped,
            "<project>\n    <build>\n        <plugins>\n            <plugin>\n                <artifactId>maven-failsafe-plugin</artifactId>\n            </plugin>\n        </plugins>\n    </build>\n</project>\n"
        );
    }

    #[test]
    fn every_older_per_set_block_goes_in_either_syntax() {
        let pom = "<project>\n    <!-- jails:generated-source-root:main -->\n    <plugin/>\n    <!-- /jails:generated-source-root:main -->\n    <!-- jails:generated-source-root:test-resources -->\n    <plugin/>\n    <!-- /jails:generated-source-root:test-resources -->\n</project>\n";
        assert_eq!(
            strip_generated_source_roots(pom).unwrap(),
            "<project>\n</project>\n"
        );
        let gradle = "plugins { id 'java' }\n\n// jails:generated-source-root:main\nsourceSets {\n    main {\n        java.srcDir('.jails/generated/main/java')\n    }\n}\n// /jails:generated-source-root:main\n";
        assert_eq!(
            strip_generated_source_roots(gradle).unwrap(),
            "plugins { id 'java' }\n\n"
        );
    }

    #[test]
    fn a_file_without_a_block_is_returned_unchanged() {
        let build = "plugins { id 'java' }\n";
        assert_eq!(strip_generated_source_roots(build).unwrap(), build);
    }

    #[test]
    fn a_damaged_block_refuses_rather_than_guessing() {
        let pom =
            "<project>\n    <!-- jails:generated-source-roots -->\n    <plugin/>\n</project>\n";
        assert!(strip_generated_source_roots(pom).is_err());
    }
}
