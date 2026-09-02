//! The two POM edits `jails modernize` makes, kept out of `pom.rs` so that
//! file stays under the largest-module ceiling.
//!
//! They belong to the same subject as the rest of `pom.rs` -- surgical edits
//! to a file the reader owns -- and are here rather than in `modernize.rs`
//! because reading a POM is this module's secret, not that one's.

use super::inside_comment;

/// The POM with `spring-boot-starter-parent` pinned to `version`.
///
/// `None` when it already is, or when there is no readable parent version --
/// a project inheriting Boot through `spring-boot-dependencies` in
/// `<dependencyManagement>` says its version somewhere this does not look, and
/// rewriting the wrong `<version>` in a POM is the worst edit available.
pub fn with_parent_version(pom: &str, version: &str) -> Option<String> {
    let at = pom.find("spring-boot-starter-parent")?;
    let start = at + pom[at..].find("<version>")? + "<version>".len();
    let end = start + pom[start..].find("</version>")?;
    if pom[start..end].trim() == version {
        return None;
    }
    let mut out = pom.to_string();
    out.replace_range(start..end, version);
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
        let open = format!("<{tag}>");
        let Some(rel) = pom.find(&open) else { continue };
        if inside_comment(pom, rel) {
            continue;
        }
        let start = rel + open.len();
        let Some(end) = pom[start..].find(&format!("</{tag}>")).map(|at| start + at) else {
            continue;
        };
        if pom[start..end].trim() == release.to_string() {
            continue;
        }
        let mut out = pom.to_string();
        out.replace_range(start..end, &release.to_string());
        return Some(out);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two version facts a Maven Spring project carries, moved together.
    #[test]
    fn the_parent_and_the_release_move_and_nothing_else_does() {
        let pom = "<project>\n  <parent>\n    \
                   <artifactId>spring-boot-starter-parent</artifactId>\n    \
                   <version>2.7.18</version>\n  </parent>\n  <properties>\n    \
                   <java.version>21</java.version>\n  </properties>\n</project>\n";
        let out = with_parent_version(pom, "4.1.0").expect("2.7.18 is not the target");
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
}
