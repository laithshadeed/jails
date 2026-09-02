//! Reading and editing a Groovy `build.gradle`, to the same standard `pom.rs`
//! reads a POM.
//!
//! ## The bar this has to clear
//!
//! The worst outcome available is a confident wrong answer -- a tool that
//! half-understands a build and reports a dependency the build does not have.
//!
//! So every reader here has three answers, not two. `Some(true)`,
//! `Some(false)`, and `None` meaning **"this file says something I do not
//! understand, so I am not going to guess."** A caller that turns `None` into
//! `false` has reintroduced exactly that failure, which is why the readers
//! that matter return `Option` rather than `bool`.
//!
//! ## What it understands, exactly
//!
//! Groovy DSL only. `build.gradle.kts` is a different language with a
//! different grammar, and a parser aimed at one that guessed at the other
//! would be the confident wrong answer in its purest form -- so a `.kts`
//! project stays [`Build::Foreign`](jails_spec::build::Build::Foreign) and
//! says so.
//!
//! Within Groovy:
//!
//! - **Dependencies** in string-notation: `implementation 'g:a'`,
//!   `implementation "g:a:v"`, and the parenthesised form. Map notation
//!   (`group: 'g', name: 'a'`) is *read* but never written, because the two
//!   spellings in one file is churn nobody asked for.
//! - **A dependency whose coordinate is not a literal** -- built from a
//!   variable, a version catalog reference, or a loop -- makes the whole
//!   question unanswerable and returns `None`. That is the single most
//!   important line in this module.
//! - **The Spring Boot plugin**, from either spelling: the modern
//!   `plugins { id 'org.springframework.boot' version '3.2.0' }` and the
//!   older `buildscript { dependencies { classpath '...:VERSION' } }` that
//!   projects generated before Gradle 2.1 still carry.
//! - **The Java release**, from `sourceCompatibility`/`targetCompatibility` or
//!   from a `java { toolchain { languageVersion = JavaLanguageVersion.of(N) } }`
//!   block. The toolchain wins when both are present, because Gradle resolves
//!   it that way.
//!
//! ## Why the edits are textual
//!
//! Same rule as `pom.rs`: this is a file the reader owns, so an edit is
//! surgical and leaves every other byte alone. There is no re-rendering of the
//! file from a model, because a model round-trip loses comments, ordering and
//! whitespace that somebody chose.

use crate::pom::{DependencyRef, Flavor};
use jails_support::Result;

/// The build file this module reads.
pub const FILE: &str = "build.gradle";

/// Groovy comments and string literals, replaced by spaces of the same length.
///
/// The same trick `java::blanked` uses and for the same reason: a scan must not
/// be fooled by a brace inside a string or a `dependencies` inside a comment,
/// while byte offsets still index the original text so a slice can be taken
/// from it. Written here rather than borrowed from `jails-java` because Groovy
/// and Java disagree about `'...'` -- a Java char literal is one character, a
/// Groovy string is any number of them, and reusing the Java scanner would
/// desynchronise on the first `'org.springframework.boot'`.
fn blanked(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = vec![b' '; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        // Triple-quoted strings first: `'''` starts with `'`, so testing the
        // single-quote case first would end the literal at the second quote.
        if rest.starts_with(b"'''") || rest.starts_with(b"\"\"\"") {
            let quote = &rest[..3];
            let mut j = i + 3;
            while j + 3 <= bytes.len() && &bytes[j..j + 3] != quote {
                j += 1;
            }
            i = (j + 3).min(bytes.len());
            continue;
        }
        match rest[0] {
            b'/' if rest.starts_with(b"//") => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if rest.starts_with(b"/*") => {
                let mut j = i + 2;
                while j + 2 <= bytes.len() && &bytes[j..j + 2] != b"*/" {
                    j += 1;
                }
                i = (j + 2).min(bytes.len());
            }
            quote @ (b'\'' | b'"') => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != quote {
                    // A backslash escapes the next byte, including a quote.
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                i = (j + 1).min(bytes.len());
            }
            other => {
                out[i] = other;
                i += 1;
            }
        }
    }
    // Newlines survive so line-oriented work still sees the same shape.
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            out[index] = b'\n';
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// The byte range of a **top-level** `name { ... }` block's body.
///
/// Top-level is the whole point: `buildscript { dependencies { ... } }` holds
/// the plugin classpath, not the project's dependencies, and a scan that found
/// the first `dependencies` would splice the application's libraries into the
/// build's own classpath -- where they change nothing and are invisible at
/// compile time.
fn top_level_body(text: &str, name: &str) -> Option<std::ops::Range<usize>> {
    let scan = blanked(text);
    let bytes = scan.as_bytes();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && at_word(&scan, i, name) => {
                // The brace that opens it, allowing `name {` and `name{`.
                let mut j = i + name.len();
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() || bytes[j] != b'{' {
                    i += 1;
                    continue;
                }
                let open = j;
                let mut inner = 0usize;
                while j < bytes.len() {
                    match bytes[j] {
                        b'{' => inner += 1,
                        b'}' => {
                            inner -= 1;
                            if inner == 0 {
                                return Some(open + 1..j);
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether `word` starts at `at` and is not part of a longer identifier.
fn at_word(text: &str, at: usize, word: &str) -> bool {
    if !text[at..].starts_with(word) {
        return false;
    }
    let before = text[..at].chars().next_back();
    let after = text[at + word.len()..].chars().next();
    let boundary =
        |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
    boundary(before) && boundary(after)
}

/// Spring Boot's dependency management, or its absence.
///
/// The same question `pom::flavor` answers about a parent POM, and it decides
/// the same thing: whether a spliced dependency may omit its version.
pub(crate) fn flavor(text: &str) -> Flavor {
    match spring_boot_major(text) {
        Some(_) => Flavor::SpringBoot,
        None => Flavor::PlainMaven,
    }
}

/// The Spring Boot major version, from either spelling of the plugin.
///
/// `None` means no Spring Boot plugin *this module can see*. Both spellings
/// are read because a reader that knows only the modern `plugins {}` block
/// reports an older `buildscript` Spring project as plain -- which changes
/// every template jails renders into it.
pub(crate) fn spring_boot_major(text: &str) -> Option<u32> {
    boot_version(text)?.split('.').next()?.parse().ok()
}

/// The Spring Boot plugin's full version string.
pub(crate) fn boot_version(text: &str) -> Option<String> {
    let span = boot_version_span(text)?;
    Some(text[span].to_string())
}

/// Where that version literal sits, so the reader and the writer cannot
/// disagree about which bytes are the version.
///
/// Two functions locating the same thing by two routes is how a splice comes
/// to rewrite something the reader never reported.
fn boot_version_span(text: &str) -> Option<std::ops::Range<usize>> {
    // Blanking finds *structure* -- which braces open which block. It is the
    // wrong tool for reading a *value*, because every value here lives inside
    // a string literal and blanking is precisely what erases those. So the
    // block is located in the blanked copy and then read out of the original,
    // which the shared offsets make safe. Backwards, every Gradle project
    // reads as plain.
    if let Some(body) = top_level_body(text, "plugins") {
        let region = &text[body.clone()];
        if let Some(at) = region.find("org.springframework.boot")
            && let Some(version_at) = region[at..].find("version")
        {
            let from = body.start + at + version_at + "version".len();
            if let Some(found) = literal_span(&text[from..]) {
                return Some(from + found.start..from + found.end);
            }
        }
    }
    // Legacy: buildscript { dependencies { classpath '...:spring-boot-gradle-plugin:V' } }
    let at = text.find("spring-boot-gradle-plugin")?;
    let from = at + "spring-boot-gradle-plugin".len();
    let tail = text[from..].strip_prefix(':')?;
    let end = tail.find(['\'', '"'])?;
    Some(from + 1..from + 1 + end)
}

/// The first string literal in `text`, unquoted.
fn first_literal(text: &str) -> Option<String> {
    let span = literal_span(text)?;
    Some(text[span].to_string())
}

/// Where the first string literal's *contents* sit, quotes excluded.
fn literal_span(text: &str) -> Option<std::ops::Range<usize>> {
    let start = text.find(['\'', '"'])?;
    let quote = text.as_bytes()[start];
    let rest = &text[start + 1..];
    let end = rest.find(quote as char)?;
    Some(start + 1..start + 1 + end)
}

/// The Java release this build targets.
///
/// A `java { toolchain { ... } }` block wins over `sourceCompatibility`,
/// because that is the order Gradle itself resolves them in -- reporting the
/// looser one would let `jails doctor` bless a project whose real toolchain is
/// older than the code jails is about to generate.
pub(crate) fn release_level(text: &str) -> Option<u32> {
    if let Some(body) = top_level_body(text, "java")
        && let Some(inner) = top_level_body(&text[body.clone()], "toolchain")
    {
        let region = &text[body.start + inner.start..body.start + inner.end];
        if let Some(at) = region.find("JavaLanguageVersion.of")
            && let Some(found) = digits(&region[at..])
        {
            return Some(found);
        }
    }
    let scan = blanked(text);
    for key in ["targetCompatibility", "sourceCompatibility"] {
        if let Some(at) = scan.find(key) {
            // `sourceCompatibility = 21`, `= '21'` and `= JavaVersion.VERSION_21`
            // all reduce to the first run of digits after the assignment.
            if let Some(found) = digits(&text[at + key.len()..]) {
                return Some(found);
            }
        }
    }
    None
}

/// The first run of ASCII digits, as a number.
fn digits(text: &str) -> Option<u32> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

// ---------------------------------------------------------------------------
// Retargeting: the four edits `jails modernize` makes to a Gradle build.
// ---------------------------------------------------------------------------

/// The Gradle wrapper's descriptor, the second file a Gradle project's version
/// facts live in.
pub const WRAPPER: &str = "gradle/wrapper/gradle-wrapper.properties";

/// The Gradle jails upgrades a project to.
///
/// Not a taste: **Gradle 8.5 does not run on JDK 26.** It is also what Spring
/// Boot's own build uses (`deps/spring-boot`'s wrapper), and Boot 4.1's system
/// requirements name "Gradle 8.x (8.14 or later) and 9.x" -- so this is the
/// intersection of what the JDK allows and what the framework supports, read
/// off the checkout rather than remembered.
pub(crate) const TARGET_GRADLE: &str = "9.7.0";

/// Whether a Gradle distribution can *launch* on a JDK release.
///
/// Not a compatibility matrix, and deliberately not one: jails has no Gradle
/// checkout under `deps/`, so a table here would be recalled rather than read,
/// and this module's bar is the one `build.rs` sets -- **answer exactly or
/// refuse, never guess.** What it holds is the pairings that were *run*, and
/// everything else is `Unknown`.
///
/// The failure this exists for is not a build error. Gradle's build-script
/// compiler rejects the launcher's own class files before any of the project
/// is read:
///
/// ```text
/// BUG! exception in phase 'semantic analysis' in source unit '_BuildScript_'
/// Unsupported class file major version 70
/// ```
///
/// Nothing in that names Gradle's version or the JDK's, and the project is
/// unbuildable rather than misconfigured -- so a tool that writes both numbers
/// must not write that pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Launches {
    /// Measured, and it runs.
    Yes,
    /// Measured, and it dies before reading the build script.
    No,
    /// Not measured. jails makes no claim, which is the answer a reader who
    /// pinned `--gradle-version` themselves is entitled to.
    Unknown,
}

/// Each pairing is measured, not recalled: `9.7.0` launches on 26; `8.5` on 26
/// dies with `Unsupported class file major version 70`, Java 26's own
/// class-file major, before any task is selected; `8.5` on 21 builds green.
///
/// The releases between 22 and 25 are **not** measured against 8.5, so they
/// are `Unknown` rather than assumed bad. Adding a row means running it.
const MEASURED: &[(&str, u32, Launches)] = &[
    ("9.7.0", 26, Launches::Yes),
    ("8.5", 26, Launches::No),
    ("8.5", 21, Launches::Yes),
];

/// Whether this distribution has been seen to launch on this release.
pub fn launches_on(distribution: &str, release: u32) -> Launches {
    MEASURED
        .iter()
        .find(|(gradle, jdk, _)| *gradle == distribution && *jdk == release)
        .map(|(_, _, answer)| *answer)
        .unwrap_or(Launches::Unknown)
}

/// The highest release this distribution is known to launch on, if any.
///
/// Used only to name a way out of a refusal -- "and `--java 21` is a pairing
/// jails has run" -- never to claim the one above it would fail.
pub fn highest_measured_release(distribution: &str) -> Option<u32> {
    MEASURED
        .iter()
        .filter(|(gradle, _, answer)| *gradle == distribution && *answer == Launches::Yes)
        .map(|(_, release, _)| *release)
        .max()
}

/// The wrapper's Gradle version, read out of its `distributionUrl`.
///
/// `None` for a URL this module cannot parse -- a mirror, a custom
/// distribution -- which is the same answer every other reader here gives to a
/// question it cannot answer exactly.
pub fn wrapper_version(text: &str) -> Option<String> {
    let span = wrapper_version_span(text)?;
    Some(text[span].to_string())
}

fn wrapper_version_span(text: &str) -> Option<std::ops::Range<usize>> {
    let at = text.find("distributions/gradle-")?;
    let from = at + "distributions/gradle-".len();
    let rest = &text[from..];
    let end = rest.find('-')?;
    Some(from..from + end)
}

/// The wrapper pointing at another Gradle version, everything else untouched.
///
/// The `-all` / `-bin` suffix is preserved: which distribution a project pulls
/// is the reader's choice (`-all` carries the sources an IDE indexes), and
/// silently swapping it would be an unrelated change riding along with this
/// one.
pub(crate) fn with_wrapper_version(text: &str, version: &str) -> Option<String> {
    let span = wrapper_version_span(text)?;
    if text[span.clone()] == *version {
        return None;
    }
    let mut out = text.to_string();
    out.replace_range(span, version);
    Some(out)
}

/// The build file with the Spring Boot plugin pinned to `version`.
///
/// `None` when it is already that version, or when the plugin's version is not
/// a literal this module can locate -- a version catalog reference, say. A
/// build whose version jails cannot *read* is one it must not rewrite.
pub(crate) fn with_boot_version(text: &str, version: &str) -> Option<String> {
    let span = boot_version_span(text)?;
    if text[span.clone()] == *version {
        return None;
    }
    let mut out = text.to_string();
    out.replace_range(span, version);
    Some(out)
}

/// The build file targeting `release`, expressed the way Gradle 9 accepts.
///
/// **`sourceCompatibility = 21` at the project level is gone in Gradle 9** --
/// `Could not set unknown property 'sourceCompatibility' for root project`, on
/// evaluation, before a single task runs. So this is not only a number change:
/// the old spelling is removed and a `java { toolchain { ... } }` block takes
/// its place, at the position the first removed line held so the file still
/// reads in the order somebody wrote it.
///
/// A project that already has a toolchain block keeps it and only the number
/// moves.
pub(crate) fn with_release_level(text: &str, release: u32) -> Option<String> {
    if let Some(body) = top_level_body(text, "java")
        && let Some(inner) = top_level_body(&text[body.clone()], "toolchain")
    {
        let region = body.start + inner.start..body.start + inner.end;
        let at = text[region.clone()].find("JavaLanguageVersion.of")?;
        let from = region.start + at;
        let start = from + text[from..region.end].find(|c: char| c.is_ascii_digit())?;
        let end = start
            + text[start..]
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(text.len() - start);
        if text[start..end] == *release.to_string() {
            return None;
        }
        let mut out = text.to_string();
        out.replace_range(start..end, &release.to_string());
        return Some(out);
    }
    let block = format!(
        "java {{\n    toolchain {{\n        languageVersion = JavaLanguageVersion.of({release})\n    }}\n}}\n"
    );
    let scan = blanked(text);
    let mut kept: Vec<&str> = Vec::new();
    let mut inserted = false;
    for (line, shape) in text.lines().zip(scan.lines()) {
        let compatibility = shape.trim_start().starts_with("sourceCompatibility")
            || shape.trim_start().starts_with("targetCompatibility");
        if !compatibility {
            kept.push(line);
            continue;
        }
        if !inserted {
            inserted = true;
            kept.push(block.trim_end());
        }
    }
    if !inserted {
        // Nothing said the release before, so there is nothing to replace and
        // the block goes at the end -- which is also the answer for a project
        // that inherits its release from a convention plugin, since one that
        // does will have the block already.
        let mut out = text.trim_end().to_string();
        out.push_str("\n\n");
        out.push_str(&block);
        return Some(out);
    }
    let mut out = kept.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// The build file with `test { useJUnitPlatform() }`, or `None` when it has it.
///
/// Gradle does not infer it, and the failure names nothing useful: *"There are
/// test sources present and no filters are applied, but the test task did not
/// discover any tests to execute."* A Boot 2 project got away without it
/// because Gradle 8's default framework found the JUnit 4 runner; a Boot 4
/// project's tests are JUnit 5 and simply do not run.
pub(crate) fn with_junit_platform(text: &str) -> Option<String> {
    if blanked(text).contains("useJUnitPlatform") {
        return None;
    }
    if let Some(body) = top_level_body(text, "test") {
        let mut out = text.to_string();
        out.insert_str(body.start, "\n    useJUnitPlatform()");
        return Some(out);
    }
    let mut out = text.trim_end().to_string();
    out.push_str("\n\ntest {\n    useJUnitPlatform()\n}\n");
    Some(out)
}

/// One dependency line this module was able to read completely.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Declared {
    group_id: String,
    artifact_id: String,
}

/// Every dependency in the top-level block, or `None` if any line is beyond
/// this reader.
///
/// **All-or-nothing on purpose.** A file where one line is
/// `implementation libs.spring.boot.starter.web` cannot be answered "no" about
/// anything, because the line jails could not read may be the very dependency
/// it was asked about. Returning the lines it *did* understand would be the
/// confident wrong answer this module exists to avoid.
fn declared(text: &str) -> Option<Vec<Declared>> {
    // No block at all is a *definite* answer -- this project declares no
    // dependencies -- and must not be confused with a block that cannot be
    // read. Returning `None` here would make a bare `plugins { id 'java' }`
    // report "cannot tell" about every dependency, which is the pessimistic
    // twin of the confident wrong answer: it refuses work that is perfectly
    // safe.
    let Some(body) = top_level_body(text, "dependencies") else {
        return Some(Vec::new());
    };
    let scan = blanked(text);
    let mut found = Vec::new();
    for (offset, line) in line_spans(&scan[body.clone()]) {
        let start = body.start + offset;
        let stripped = line.trim();
        if stripped.is_empty() || stripped == "}" {
            continue;
        }
        let Some(configuration) = stripped.split(['(', ' ', '\t']).next() else {
            continue;
        };
        if !is_configuration(configuration) {
            // A `constraints {}` brace, a conditional, an `exclude` -- not a
            // dependency declaration, and not this reader's business.
            if stripped.starts_with('}') || stripped.ends_with('{') {
                continue;
            }
            return None;
        }
        let original = &text[start..start + line.len()];
        // A configuration jails recognises, declaring something it cannot read
        // -- a variable, a version catalog alias, a project reference -- makes
        // the whole file unanswerable rather than this one line skippable.
        found.push(coordinate_of(original)?);
    }
    Some(found)
}

/// Byte offset and text of each line.
fn line_spans(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split('\n').map(move |line| {
        let start = at;
        at += line.len() + 1;
        (start, line)
    })
}

/// Gradle configurations jails knows how to read and write.
fn is_configuration(word: &str) -> bool {
    matches!(
        word,
        "implementation"
            | "api"
            | "compileOnly"
            | "runtimeOnly"
            | "testImplementation"
            | "testCompileOnly"
            | "testRuntimeOnly"
            | "developmentOnly"
            | "annotationProcessor"
            | "testAnnotationProcessor"
    )
}

/// `group:artifact` out of one declaration, in either notation.
fn coordinate_of(line: &str) -> Option<Declared> {
    // Map notation: group: 'g', name: 'a'
    if let Some(group_at) = line.find("group:").or_else(|| line.find("group :")) {
        let group = first_literal(&line[group_at..])?;
        let name_at = line.find("name:").or_else(|| line.find("name :"))?;
        let artifact = first_literal(&line[name_at..])?;
        return Some(Declared {
            group_id: group,
            artifact_id: artifact,
        });
    }
    // String notation: 'group:artifact' or "group:artifact:version"
    let literal = first_literal(line)?;
    let mut parts = literal.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    Some(Declared {
        group_id: group.to_string(),
        artifact_id: artifact.to_string(),
    })
}

/// Whether this build declares a dependency, or `None` when it cannot be told.
///
/// The `Option` is the contract. `pom::has_dependency` returns `bool` because
/// a POM is XML and either has the element or does not; a Gradle build can
/// compute its dependency list, and "I could not read line 14" is a different
/// answer from "it is not there".
pub(crate) fn has_dependency(text: &str, group_id: &str, artifact_id: &str) -> Option<bool> {
    Some(
        declared(text)?
            .iter()
            .any(|one| one.group_id == group_id && one.artifact_id == artifact_id),
    )
}

/// The Gradle configuration a Maven scope means.
///
/// `optional` maps to `developmentOnly` rather than `compileOnly`, because the
/// dependencies jails marks optional are runtime conveniences --
/// `spring-boot-docker-compose` is the case -- and `compileOnly` would leave
/// them off the runtime classpath where they do their whole job.
fn configuration_for(scope: Option<&str>, optional: bool) -> &'static str {
    if optional {
        return "developmentOnly";
    }
    match scope {
        Some("test") => "testImplementation",
        Some("runtime") => "runtimeOnly",
        Some("provided") => "compileOnly",
        _ => "implementation",
    }
}

/// Splice one dependency into the top-level `dependencies` block.
///
/// `Ok(None)` means "already there, nothing to do", matching
/// `pom::add_dependency_ref` so the projection can treat both build files the
/// same way. An unreadable dependency block is an `Err`, never a silent
/// append: appending to a file jails does not understand is how a duplicate
/// declaration with a different version gets in.
pub fn add_dependency_ref(text: &str, dep: DependencyRef<'_>) -> Result<Option<String>> {
    match has_dependency(text, dep.group_id, dep.artifact_id) {
        Some(true) => return Ok(None),
        Some(false) => {}
        None => {
            return Err(format!(
                "`{FILE}` declares dependencies this jails cannot read, so it will not add \
                 `{}:{}` beside them.\n       fix: add it by hand. jails refuses here rather \
                 than appending, because a build that computes its dependency list may already \
                 have this one under another spelling -- and two declarations at different \
                 versions is the failure that costs an afternoon.",
                dep.group_id, dep.artifact_id
            )
            .into());
        }
    }
    let coordinate_for_new_block = match dep.version {
        Some(version) => format!("{}:{}:{version}", dep.group_id, dep.artifact_id),
        None => format!("{}:{}", dep.group_id, dep.artifact_id),
    };
    let Some(body) = top_level_body(text, "dependencies") else {
        // A build with no `dependencies` block at all -- a bare
        // `plugins { id 'java' }` is the ordinary case. Appending the block is
        // still a surgical edit: it adds, and changes nothing that was there.
        // Refusing instead would make `add` unusable on exactly the projects
        // that have not needed a library yet.
        let separator = match text.ends_with('\n') || text.is_empty() {
            true => "",
            false => "\n",
        };
        return Ok(Some(format!(
            "{text}{separator}\ndependencies {{\n    {} '{coordinate_for_new_block}'\n}}\n",
            configuration_for(dep.scope, dep.optional)
        )));
    };
    let coordinate = match dep.version {
        Some(version) => format!("{}:{}:{version}", dep.group_id, dep.artifact_id),
        None => format!("{}:{}", dep.group_id, dep.artifact_id),
    };
    let inline = !text[body.clone()].contains('\n');
    let line = format!(
        "{}{}{} '{coordinate}'\n",
        if inline { "\n" } else { "" },
        indent_of(text, body.clone()),
        configuration_for(dep.scope, dep.optional)
    );
    // Inserted before the closing brace, so it lands last in the block and
    // every existing byte -- comments, ordering, blank lines -- is untouched.
    let mut out = String::with_capacity(text.len() + line.len());
    let at = if inline {
        body.end
    } else {
        trailing_insert_point(text, body)
    };
    out.push_str(&text[..at]);
    out.push_str(&line);
    out.push_str(&text[at..]);
    Ok(Some(out))
}

/// The indentation the block's existing lines use, so a spliced line matches.
fn indent_of(text: &str, body: std::ops::Range<usize>) -> String {
    for line in text[body].lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !indent.is_empty() {
            return indent;
        }
    }
    "    ".to_string()
}

/// Where a new line goes: after the block's last non-blank line.
fn trailing_insert_point(text: &str, body: std::ops::Range<usize>) -> usize {
    let region = &text[body.clone()];
    match region.rfind(|c: char| !c.is_whitespace()) {
        Some(at) => {
            let absolute = body.start + at + 1;
            // Past the newline that ends that line, so the insert is a whole
            // line rather than a continuation of somebody else's.
            match text[absolute..].find('\n') {
                Some(newline) => absolute + newline + 1,
                None => absolute,
            }
        }
        None => body.start,
    }
}

/// The class the packaged jar starts, if the build names one.
///
/// Both spellings, for the same reason both plugin spellings are read:
/// `bootJar { mainClass = '...' }` is the modern one and
/// `springBoot { mainClass = '...' }` is what older builds carry.
pub(crate) fn main_class(text: &str) -> Option<String> {
    for block in ["bootJar", "springBoot", "application"] {
        if let Some(body) = top_level_body(text, block) {
            let region = &blanked(text)[body.clone()];
            for key in ["mainClass", "mainClassName"] {
                if let Some(at) = region.find(key)
                    && let Some(found) = first_literal(&text[body.start + at..])
                {
                    return Some(found);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Boot 2 build in the older `buildscript` spelling. Every field it
    /// exercises is one a template reads.
    const MINICOM: &str = r#"buildscript {
    repositories {
        mavenCentral()
    }
    dependencies {
        classpath("org.springframework.boot:spring-boot-gradle-plugin:2.7.18")
    }
}

apply plugin: 'java'
apply plugin: 'org.springframework.boot'

bootJar {
    archiveBaseName = 'gs-rest-service'
    archiveVersion =  '0.1.0'
}

repositories {
    mavenCentral()
}

sourceCompatibility = 21
targetCompatibility = 21

dependencies {
	implementation 'org.springframework.boot:spring-boot-starter-data-jdbc'
	runtimeOnly 'com.h2database:h2'
    implementation("org.springframework.boot:spring-boot-starter-web")
    testImplementation('org.springframework.boot:spring-boot-starter-test')
}
"#;

    /// The four edits, in the order `./gradlew build` on JDK 26 fails without
    /// them: unknown property `sourceCompatibility`, then no tests
    /// discovered. The versions are checked rather than assumed -- reading the
    /// result back through this module's own readers is what stops a splice
    /// that lands somewhere the reader never looks.
    #[test]
    fn the_four_edits_that_take_a_boot_2_gradle_build_to_jdk_26() {
        let out = with_boot_version(MINICOM, "4.1.0").expect("the legacy spelling is rewritable");
        assert_eq!(boot_version(&out).as_deref(), Some("4.1.0"));
        assert!(out.contains("spring-boot-gradle-plugin:4.1.0"), "{out}");
        // Idempotent: a second run has nothing to say.
        assert!(with_boot_version(&out, "4.1.0").is_none());

        let out = with_release_level(&out, 26).expect("21 is not 26");
        assert_eq!(release_level(&out), Some(26));
        // The old spelling is *removed*, not left beside the new one: Gradle 9
        // fails evaluation on it, so leaving it would make the file worse than
        // before the upgrade.
        assert!(!out.contains("sourceCompatibility"), "{out}");
        assert!(!out.contains("targetCompatibility"), "{out}");
        assert!(
            out.contains("        languageVersion = JavaLanguageVersion.of(26)"),
            "{out}"
        );
        // In the position the removed lines held, so the file still reads in
        // the order somebody wrote it -- `rfind`, because the *first*
        // `dependencies {` in this file is the buildscript's plugin classpath.
        assert!(
            out.find("java {").unwrap() < out.rfind("dependencies {").unwrap(),
            "{out}"
        );
        assert!(with_release_level(&out, 26).is_none());

        let out = with_junit_platform(&out).expect("minicom has no test block");
        assert!(out.contains("test {\n    useJUnitPlatform()\n}"), "{out}");
        assert!(with_junit_platform(&out).is_none());
    }

    /// A project that already says it in the modern spelling gets the number
    /// changed and nothing else.
    #[test]
    fn a_toolchain_block_is_retargeted_rather_than_duplicated() {
        let text = "java {\n    toolchain {\n        languageVersion = \
                    JavaLanguageVersion.of(21)\n    }\n}\n";
        let out = with_release_level(text, 26).expect("21 is not 26");
        assert_eq!(out.matches("toolchain").count(), 1, "{out}");
        assert_eq!(release_level(&out), Some(26));
    }

    /// The suffix is the reader's choice and survives. `-all` carries the
    /// sources an IDE indexes; swapping it to `-bin` would be an unrelated
    /// change riding along with the upgrade.
    #[test]
    fn the_wrapper_moves_version_and_keeps_its_distribution() {
        let text =
            "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.5-all.zip\n";
        assert_eq!(wrapper_version(text).as_deref(), Some("8.5"));
        let out = with_wrapper_version(text, TARGET_GRADLE).expect("8.5 is not the target");
        assert!(out.ends_with("gradle-9.7.0-all.zip\n"), "{out}");
        assert!(with_wrapper_version(&out, TARGET_GRADLE).is_none());
    }

    /// A version this module cannot read is one it must not rewrite. The
    /// single most important line in the module, applied to the writers.
    #[test]
    fn a_version_catalog_reference_is_refused_rather_than_guessed_at() {
        let text = "plugins {\n    id 'org.springframework.boot' version libs.versions.boot\n}\n";
        assert_eq!(boot_version(text), None);
        assert_eq!(with_boot_version(text, "4.1.0"), None);
        assert_eq!(
            wrapper_version("distributionUrl=https\\://mirror/gradle.zip"),
            None
        );
    }

    /// The whole point of `top_level_body`: `buildscript` has a `dependencies`
    /// block too, and it holds the plugin classpath. Splicing the
    /// application's libraries in there puts them on the build's classpath,
    /// where they change nothing and are invisible at compile time.
    #[test]
    fn the_buildscript_dependencies_block_is_not_the_projects() {
        let found = declared(MINICOM).expect("every line is readable");
        assert!(
            !found
                .iter()
                .any(|one| one.artifact_id == "spring-boot-gradle-plugin"),
            "{found:?}"
        );
        assert_eq!(found.len(), 4, "{found:?}");
    }

    #[test]
    fn dependencies_are_read_in_every_notation_the_file_uses() {
        assert_eq!(
            has_dependency(MINICOM, "com.h2database", "h2"),
            Some(true),
            "bare string notation"
        );
        assert_eq!(
            has_dependency(
                MINICOM,
                "org.springframework.boot",
                "spring-boot-starter-web"
            ),
            Some(true),
            "parenthesised notation"
        );
        assert_eq!(
            has_dependency(MINICOM, "org.assertj", "assertj-core"),
            Some(false)
        );
    }

    /// A reader that only knew `plugins {}` would report an older
    /// `buildscript` Spring project as plain, which changes every template
    /// jails renders into it.
    #[test]
    fn the_boot_version_is_read_from_the_legacy_buildscript_spelling() {
        assert_eq!(boot_version(MINICOM).as_deref(), Some("2.7.18"));
        assert_eq!(spring_boot_major(MINICOM), Some(2));
        assert_eq!(flavor(MINICOM), Flavor::SpringBoot);
    }

    #[test]
    fn the_boot_version_is_read_from_the_modern_plugins_block() {
        let modern =
            "plugins {\n    id 'java'\n    id 'org.springframework.boot' version '3.2.0'\n}\n";
        assert_eq!(boot_version(modern).as_deref(), Some("3.2.0"));
        assert_eq!(spring_boot_major(modern), Some(3));
    }

    #[test]
    fn a_build_with_no_spring_boot_plugin_is_plain() {
        let plain = "plugins {\n    id 'java'\n}\ndependencies {\n}\n";
        assert_eq!(spring_boot_major(plain), None);
        assert_eq!(flavor(plain), Flavor::PlainMaven);
    }

    #[test]
    fn the_release_level_is_read_from_source_compatibility() {
        assert_eq!(release_level(MINICOM), Some(21));
    }

    /// Gradle resolves the toolchain over `sourceCompatibility`, so reporting
    /// the looser one would bless a project whose real toolchain is older than
    /// the code jails is about to generate.
    #[test]
    fn a_toolchain_block_wins_over_source_compatibility() {
        let both = "sourceCompatibility = 17\njava {\n    toolchain {\n        languageVersion = JavaLanguageVersion.of(21)\n    }\n}\n";
        assert_eq!(release_level(both), Some(21));
    }

    /// The single most important behaviour in this module. One line it cannot
    /// read makes *every* answer about the file `None`, because the line it
    /// could not read may be the very dependency it was asked about.
    #[test]
    fn one_unreadable_line_makes_the_whole_question_unanswerable() {
        let catalog = "dependencies {\n    implementation libs.spring.boot.starter.web\n    runtimeOnly 'com.h2database:h2'\n}\n";
        assert_eq!(declared(catalog), None);
        assert_eq!(has_dependency(catalog, "com.h2database", "h2"), None);
        assert_eq!(
            has_dependency(catalog, "org.assertj", "assertj-core"),
            None,
            "and especially not `false`, which is the confident wrong answer"
        );
    }

    /// Refusing beats appending: a build that computes its list may already
    /// have the dependency under a spelling jails cannot see, and two
    /// declarations at different versions is the failure that costs an
    /// afternoon.
    #[test]
    fn splicing_into_an_unreadable_block_refuses_rather_than_appending() {
        let catalog = "dependencies {\n    implementation libs.spring.boot.starter.web\n}\n";
        let error = add_dependency_ref(catalog, assertj_ref()).unwrap_err();
        assert!(error.contains("cannot read"), "{error}");
        assert!(error.contains("fix:"), "{error}");
    }

    fn assertj_ref() -> DependencyRef<'static> {
        DependencyRef {
            group_id: "org.assertj",
            artifact_id: "assertj-core",
            version: None,
            scope: Some("test"),
            optional: false,
        }
    }

    #[test]
    fn a_spliced_dependency_lands_in_the_project_block_with_its_configuration() {
        let out = add_dependency_ref(MINICOM, assertj_ref()).unwrap().unwrap();
        assert!(
            out.contains("testImplementation 'org.assertj:assertj-core'"),
            "{out}"
        );
        // Inside the project block, not the buildscript one.
        let body = top_level_body(&out, "dependencies").unwrap();
        assert!(out[body].contains("assertj-core"));
        assert_eq!(
            has_dependency(&out, "org.assertj", "assertj-core"),
            Some(true)
        );
    }

    #[test]
    fn a_single_line_dependencies_block_is_spliced_in_place_and_idempotent() {
        let build = "plugins { id 'java' }\ndependencies { implementation 'org.springframework.boot:spring-boot-starter-web' }\n";
        let once = add_dependency_ref(build, assertj_ref()).unwrap().unwrap();
        let body = top_level_body(&once, "dependencies").unwrap();
        assert!(once[body].contains("testImplementation 'org.assertj:assertj-core'"));
        assert_eq!(once.matches("assertj-core").count(), 1, "{once}");
        assert_eq!(add_dependency_ref(&once, assertj_ref()).unwrap(), None);
    }

    /// Same contract as `pom::add_dependency_ref`, so the projection can treat
    /// the two build files identically.
    /// "There is no block" and "I cannot read the block" are different
    /// answers, and collapsing them costs in both directions: as `None` it
    /// refuses safe work, as `Some(false)` it would claim a dependency is
    /// absent from a file it could not read.
    #[test]
    fn no_dependencies_block_is_absence_and_an_unreadable_one_is_not() {
        let bare = "plugins {\n    id 'java'\n}\n";
        assert_eq!(
            has_dependency(bare, "org.assertj", "assertj-core"),
            Some(false)
        );
        let catalog = "dependencies {\n    implementation libs.assertj\n}\n";
        assert_eq!(has_dependency(catalog, "org.assertj", "assertj-core"), None);
    }

    /// A bare `plugins { id 'java' }` is the ordinary shape of a project that
    /// has not needed a library yet, and refusing there would make `add`
    /// unusable on exactly those. Appending a block is still surgical: it adds
    /// and changes nothing that was already written.
    #[test]
    fn a_build_with_no_dependencies_block_gets_one() {
        let bare = "plugins {\n    id 'java'\n}\n";
        let out = add_dependency_ref(bare, assertj_ref()).unwrap().unwrap();
        assert!(out.starts_with("plugins {\n    id 'java'\n}\n"), "{out}");
        assert_eq!(
            has_dependency(&out, "org.assertj", "assertj-core"),
            Some(true),
            "{out}"
        );
    }

    #[test]
    fn splicing_something_already_there_changes_nothing() {
        assert_eq!(
            add_dependency_ref(
                MINICOM,
                DependencyRef {
                    group_id: "com.h2database",
                    artifact_id: "h2",
                    version: None,
                    scope: Some("runtime"),
                    optional: false,
                }
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn every_other_byte_survives_a_splice() {
        let out = add_dependency_ref(MINICOM, assertj_ref()).unwrap().unwrap();
        assert!(out.contains("archiveBaseName = 'gs-rest-service'"));
        assert!(
            out.contains(
                "\timplementation 'org.springframework.boot:spring-boot-starter-data-jdbc'"
            )
        );
        assert!(out.starts_with("buildscript {"));
    }

    /// A brace inside a string must not open a block, and a `dependencies`
    /// inside a comment must not be found.
    #[test]
    fn a_brace_in_a_string_and_a_block_in_a_comment_are_both_ignored() {
        let tricky = "// dependencies { implementation 'a:b' }\ntask x {\n    doLast { println '{' }\n}\ndependencies {\n    runtimeOnly 'com.h2database:h2'\n}\n";
        let found = declared(tricky).expect("readable");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].artifact_id, "h2");
    }
}

// ---------------------------------------------------------------------------
// Build features: what a Maven plugin *does*, said in Gradle
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feature_tests {}
