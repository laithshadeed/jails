//! `jails` end to end: the real compiled binary against real directories.
//!
//! One test binary, five subjects. It was one 8,142-line file, which is not a
//! size anybody navigates -- `pending.md` §8.2. The split is by *subject*
//! rather than by tier, because which tier a test belongs to is already
//! visible in whether it calls `common::skip`, while what a test is about was
//! visible nowhere.
//!
//! **One binary, not five.** Every `tests/*.rs` cargo finds is its own crate,
//! its own link and its own process; five targets would pay the link five
//! times and could not share a fixture. This is one target with the subjects
//! as ordinary submodules, so the helpers below stay defined once and each
//! subject reaches them through `use super::*`.
//!
//! `common/` is shared with the other test binaries, so it stays beside them
//! rather than moving under this one -- hence the `#[path]`.

#[path = "../common/mod.rs"]
mod common;

mod app;
mod behavior_matrix;
mod capabilities;
mod developer_tools;
mod editor_protocol;
mod effects;
mod examples;
mod generate;
mod history;
mod model;
mod new;
mod portable_plan;
mod reports;
mod sql;
mod tooling;

use common::*;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

// ---- offline, filesystem-only: exercise the real binary against real
// temp dirs, no Maven involved. ----

/// Pulls the `opts="..."` line right after a `<marker>)` case arm.
fn opts_line_for<'a>(script: &'a str, marker: &str) -> &'a str {
    let start = script
        .find(marker)
        .unwrap_or_else(|| panic!("marker {marker} not found in completion script"));
    script[start..]
        .lines()
        .find(|l| l.trim_start().starts_with("opts="))
        .unwrap()
}

/// Every file under a directory with its bytes, so "left it alone" is a claim
/// about content rather than only about which names still exist.
fn snapshot_tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(snapshot_tree(&path));
        } else {
            out.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    out.sort();
    out
}

/// The proof applications, as (name, manifest). One list, read by both gates
/// below: a second copy is how one of them quietly stops covering an app.
///
/// The ledger CLI is **not** here -- it is the control, has no Spring parent,
/// and needs the plain fixture. `ledger_cli_manifest_builds_without_spring`
/// is its gate.
const SPRING_APP_MANIFESTS: &[(&str, &str)] = &[
    (
        "web-crawler",
        include_str!("../../examples/web-crawler/.jails/app.toml"),
    ),
    (
        "support-inbox",
        include_str!("../../examples/support-inbox/.jails/app.toml"),
    ),
    (
        "payments-gateway",
        include_str!("../../examples/payments-gateway/.jails/app.toml"),
    ),
];

const PROOF_APP_CACHE_SCHEMA: &str = "proof-apps:v2:shared-demo-actuator-prometheus-context";

/// Finish only the concrete toolbox proof after the generic generators have
/// written their intentionally honest TODOs.
fn overlay_plain_toolbox_completions(root: &Path) {
    const FILES: &[&str] = &[
        "src/main/java/com/example/demo/MoneyMoved.java",
        "src/main/java/com/example/demo/domain/Tally.java",
        "src/main/java/com/example/demo/domain/Transaction.java",
        "src/main/java/com/example/demo/service/DomesticEligibility.java",
        "src/main/java/com/example/demo/service/ExactReferenceMatchRule.java",
        "src/main/java/com/example/demo/service/AmountAndDateMatchRule.java",
        "src/main/java/com/example/demo/service/FuzzyMemoMatchRule.java",
        "src/test/java/com/example/demo/MoneyMovedTest.java",
        "src/test/java/com/example/demo/domain/TallyTest.java",
        "src/test/java/com/example/demo/service/DomesticEligibilityTest.java",
        "src/test/java/com/example/demo/service/ExactReferenceMatchRuleTest.java",
        "src/test/java/com/example/demo/service/AmountAndDateMatchRuleTest.java",
        "src/test/java/com/example/demo/service/FuzzyMemoMatchRuleTest.java",
        "src/test/java/com/example/demo/BriefTest.java",
        "src/test/java/com/example/demo/CheckoutIT.java",
    ];
    let fixtures = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plain-toolbox-completions"
    ));

    for relative in FILES {
        let source = fixtures.join(relative);
        let destination = root.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to overlay {} onto {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }
}

/// One real plain-Maven verification for every plain capability and generator
/// branch exercised below. The focused Rust tests still generate their own
/// projects and assert their exact semantics; sharing only this compile/test
/// gate removes repeated Maven/JUnit startup without dropping a source or a
/// generated test from toolchain coverage.
fn verified_plain_toolbox(path: &str) -> &'static std::path::PathBuf {
    static VERIFIED: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let workdir = temp_dir("plain-toolbox-verified");
        let status = jails_cmd_with_path(&workdir, path)
            .args(["new-cli", "demo"])
            .status()
            .unwrap();
        assert!(status.success(), "new-cli failed for the plain toolbox");
        let root = workdir.join("demo");
        for capability in ["fake", "http"] {
            let status = jails_cmd_with_path(&root, path)
                .args(["add", capability])
                .status()
                .unwrap();
            assert!(status.success(), "add {capability} failed in plain toolbox");
        }
        for args in [
            &["g", "class", "MoneyMoved"][..],
            &[
                "g",
                "record",
                "Tally",
                "hits:int@nonnegative",
                "total:long@nonnegative",
            ][..],
            &["g", "enum", "Currency", "GBP", "EUR"][..],
            &[
                "g",
                "record",
                "SourceRef",
                "system:string",
                "externalId:string",
            ][..],
            &[
                "g",
                "value",
                "CanonicalTransaction",
                "id:string!",
                "date:date",
                "amountMinor:long",
                "currency:Currency",
                "source:SourceRef",
                "note:string?",
            ][..],
            &["g", "sealed", "Outcome", "Accepted", "Rejected"][..],
            &[
                "g",
                "value",
                "Stamped",
                "at:string!",
                "result:Outcome",
            ][..],
            &["g", "record", "Transaction", "id:uuid", "amount:long"][..],
            &["g", "record", "Reward", "id:uuid", "amount:long"][..],
            &[
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction",
            ][..],
            &["g", "integration-test", "Checkout"][..],
        ] {
            let status = jails_cmd_with_path(&root, path)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "{args:?} failed in plain toolbox");
        }
        fs::write(
            root.join("brief.md"),
            "# Brief\n\n## Acceptance criteria\n\n- parses a `quoted` value\n- rejects **blank** ids\n",
        )
        .unwrap();
        let status = jails_cmd_with_path(&root, path)
            .args(["g", "cases", "brief.md"])
            .status()
            .unwrap();
        assert!(status.success(), "generate cases failed in plain toolbox");

        // Apply the exact control manifest last. Its deferred `format`
        // capability formats both the manifest output and the toolbox files
        // above in one invocation, after every source exists.
        fs::create_dir_all(root.join(".jails")).unwrap();
        fs::write(
            root.join(".jails/app.toml"),
            include_str!("../../examples/ledger-cli/.jails/app.toml"),
        )
        .unwrap();
        let output = jails_cmd_with_path(&root, path)
            .args(["app", "apply", "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "plain toolbox manifest: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // The ledger manifest makes LedgerCli the executable dispatcher.
        // Generate this command afterwards and target that dispatcher so the
        // shared runtime gate proves the final application registration.
        let status = jails_cmd_with_path(&root, path)
            .args(["g", "command", "Greet", "--on", "LedgerCli"])
            .status()
            .unwrap();
        assert!(status.success(), "generate Greet failed in plain toolbox");

        overlay_plain_toolbox_completions(&root);

        let status = jails_cmd_with_path(&root, path)
            .arg("check")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "the shared plain toolbox failed clean verify"
        );
        let surefire = maven_report_summary(&root, "surefire-reports");
        assert_eq!(
            surefire,
            MavenReportSummary {
                reports: 29,
                tests: 89,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the plain toolbox must execute every Surefire test"
        );
        let failsafe = maven_report_summary(&root, "failsafe-reports");
        assert_eq!(
            failsafe,
            MavenReportSummary {
                reports: 1,
                tests: 1,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the plain toolbox must execute CheckoutIT"
        );
        root
    })
}

/// Two concurrent Spring/JUnit executions for the focused capability and
/// generator tests.
/// Each Rust test still creates its own fixture and checks the exact files it
/// asked jails to write; these toolboxes are the shared proof that the same
/// generated branches compile and that every Surefire test actually runs.
///
/// The split is semantic, not a test filter: security/Redis/mail change the
/// actuator health result, and SSE plus a job deliberately produce separate
/// SchedulingConfig classes which would collide in one artificial app. Every
/// generated test in both valid projects runs, and their Maven lifecycles
/// overlap.
struct SpringToolboxes {
    core: std::path::PathBuf,
    services: std::path::PathBuf,
}

fn verified_spring_toolboxes(path: &str) -> &'static SpringToolboxes {
    static VERIFIED: std::sync::OnceLock<SpringToolboxes> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        // Salted with this file's own text, not just the product binary.
        // A toolbox is a *harness* input: adding a generator to the list below
        // changes what the shared project proves, and without the salt the
        // cached tree from the previous run is reused and the new step never
        // runs -- a control that reports green over commands it did not
        // execute. Same failure as a skipped tier-3 test, one level up.
        let salt = include_str!("main.rs");
        let (core, core_fresh) = cached_toolchain_dir_with_salt("spring-core-toolbox", salt);
        let (services, services_fresh) =
            cached_toolchain_dir_with_salt("spring-services-toolbox", salt);

        if core_fresh {
            write_spring_fixture(&core);
            // `cors` and `h2` are here because their templates are chosen by
            // the project's Boot version and only the *legacy* branch was ever
            // compiled: `add cors` is run through real `mvn test` against a
            // Boot 2 fixture, and `add h2`'s Boot 4 branch adds a console
            // module that had no test at all. A tier-3 test pinned to the old
            // branch reports green for the branch every real project gets.
            for capability in [
                "api",
                "cache",
                "actuator",
                "observability",
                "json",
                "sse",
                "cors",
                "h2",
            ] {
                let status = jails_cmd_with_path(&core, path)
                    .args(["add", capability, "--no-start"])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "add {capability} failed in the core Spring toolbox"
                );
            }
            for args in [
                &[
                    "generate",
                    "scaffold",
                    "Post",
                    "id:uuid@pk",
                    "title:string",
                    "body:text",
                    "published:boolean",
                ][..],
                &["generate", "controller", "Health"][..],
                &["generate", "service", "Billing"][..],
                &[
                    "generate",
                    "record",
                    "Tag",
                    "name:string",
                    "createdAt:datetime",
                ][..],
                &[
                    "generate",
                    "record",
                    "Payout",
                    "id:uuid",
                    "amount:long",
                    "note:string?",
                ][..],
                &["generate", "dto", "Payout"][..],
                &["generate", "client", "Billing"][..],
                // The other client shape: the call the project makes rather
                // than a REST collection to delete (missing.md M7). Its test
                // is `@Disabled` and still has to compile.
                &[
                    "generate",
                    "client",
                    "Ledger",
                    "--method",
                    "post",
                    "--on",
                    "Payout",
                    "--path",
                    "/v1/ledger/entries",
                ][..],
                // Two kinds that only contradict each other in company.
                // `g scaffold` writes an ArchUnit rule forbidding
                // `org.springframework..` inside `domain..`, and `g strategy`
                // wrote `@Component` implementations into it -- a red build on
                // a clean generate, invisible to every scenario in the suite
                // because each exercises one kind in one project. This toolbox
                // is where a first-party generator meets the fitness function
                // another first-party generator installed.
                // The WebSocket half. Its handler and test are ordinary Java,
                // but the starter `g socket` splices is what decides whether
                // `org.springframework.web.socket` resolves at all -- and a
                // generator that emits code and not its dependency hands the
                // reader a compile error for a line they did not write.
                &["generate", "socket", "Chat"][..],
                &["generate", "enum", "Visibility", "PUBLIC", "PRIVATE"][..],
                &[
                    "generate", "strategy", "PostRule", "Featured", "Standard", "--on", "Post",
                    "--yields", "Tag",
                ][..],
            ] {
                let status = jails_cmd_with_path(&core, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "{args:?} failed in the core Spring toolbox"
                );
            }

            mark_toolchain_dir_generated(&core);
        }

        if services_fresh {
            write_spring_fixture(&services);
            for capability in ["kafka", "security", "redis", "mail"] {
                let status = jails_cmd_with_path(&services, path)
                    .args(["add", capability, "--no-start"])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "add {capability} failed in the services Spring toolbox"
                );
            }
            for args in [
                &[
                    "generate",
                    "event",
                    "PayoutSettled",
                    "id:uuid",
                    "payoutId:uuid",
                    "amount:decimal",
                    "occurredAt:instant",
                ][..],
                &["generate", "auth", "Api"][..],
                &["generate", "webhook", "Provider"][..],
                &["generate", "job", "Sweep"][..],
            ] {
                let status = jails_cmd_with_path(&services, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "{args:?} failed in the services Spring toolbox"
                );
            }
            mark_toolchain_dir_generated(&services);
        }

        std::thread::scope(|scope| {
            let core_test = scope.spawn(|| {
                real_maven_cmd(&core, path)
                    .args(["-q", "test"])
                    .status()
                    .unwrap()
            });
            let services_test = scope.spawn(|| {
                real_maven_cmd(&services, path)
                    .args([
                        "-q",
                        "-Dapp.auth.secret=0123456789abcdef0123456789abcdef",
                        "-Dapp.provider.secret=toolbox-provider-secret",
                        "test",
                    ])
                    .status()
                    .unwrap()
            });
            assert!(
                core_test.join().unwrap().success(),
                "the core Spring toolbox failed mvn test"
            );
            assert!(
                services_test.join().unwrap().success(),
                "the services Spring toolbox failed mvn test"
            );
        });
        SpringToolboxes { core, services }
    })
}

fn verified_spring_toolbox(path: &str) -> &'static std::path::PathBuf {
    &verified_spring_toolboxes(path).core
}

fn verified_spring_services_toolbox(path: &str) -> &'static std::path::PathBuf {
    &verified_spring_toolboxes(path).services
}

/// Shared compile-and-unit-test proof for generators which require the JDBC
/// capability. The dedicated `add_db_on_spring_makes_context_loads_pass` test
/// still exercises the generated Testcontainers default against PostgreSQL;
/// this toolbox uses H2 for the branches whose original contract was only
/// javac/Surefire, avoiding three more Maven JVMs and containers.
fn verified_spring_db_toolbox(path: &str) -> &'static std::path::PathBuf {
    static VERIFIED: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let (root, fresh) =
            cached_toolchain_dir_with_salt("spring-db-toolbox", include_str!("main.rs"));
        if fresh {
            write_spring_fixture(&root);
            let status = jails_cmd_with_path(&root, path)
                .args(["add", "db", "--no-start"])
                .status()
                .unwrap();
            assert!(status.success(), "add db failed in the JDBC toolbox");

            for args in [
                &["generate", "enum", "Currency", "GBP", "USD"][..],
                &[
                    "generate",
                    "scaffold",
                    "Payout",
                    "id:uuid@pk",
                    "amount:bigdecimal",
                    "currency:Currency",
                    "paidAt:instant",
                    "note:string?",
                ][..],
                &["generate", "idempotency", "Request"][..],
                &[
                    "generate",
                    "scaffold",
                    "Article",
                    "id:uuid@pk",
                    "title:string!",
                    "body:string",
                ][..],
                &["generate", "search", "Article", "title", "body"][..],
                // The form-bound pair. They are here rather than in a test of
                // their own because this fixture already runs `mvn test`, and
                // a real build is the only oracle for the defect they cover:
                // every `--consumes form` endpoint jails wrote shipped a proof
                // that posted a JSON body at an `@ModelAttribute` parameter,
                // which binds from request *parameters*. The goldens were
                // green over it -- they compare bytes and never run the code.
                &[
                    "generate",
                    "scaffold",
                    "Note",
                    "id:long@pk",
                    "body:string!",
                    "seen:boolean",
                    "version:long",
                ][..],
                &[
                    "generate",
                    "usecase",
                    "PostNote",
                    "body:string!",
                    "--on",
                    "Note",
                    "--consumes",
                    "form",
                ][..],
                &[
                    "generate",
                    "transition",
                    "MarkNoteSeen",
                    "id:long",
                    "version:long",
                    "--on",
                    "Note",
                    "--set",
                    "seen=true",
                    "--if-match",
                    "optional",
                    "--consumes",
                    "form",
                ][..],
            ] {
                let status = jails_cmd_with_path(&root, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(status.success(), "{args:?} failed in the JDBC toolbox");
            }

            add_app_unit_test_database(&root);
            mark_toolchain_dir_generated(&root);
        }
        let mut command = real_maven_cmd(&root, path);
        configure_app_unit_maven(&mut command, "db-toolbox");
        let status = command.args(["-q", "test"]).status().unwrap();
        assert!(status.success(), "the shared JDBC toolbox failed mvn test");
        root
    })
}

fn assert_surefire_test_count(root: &Path, class_name: &str, expected: usize) {
    let reports = root.join("target/surefire-reports");
    let report = fs::read_dir(&reports)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("TEST-") && name.contains(class_name) && name.ends_with(".xml")
        })
        .unwrap_or_else(|| panic!("{class_name} did not produce a Surefire XML report"));
    let xml = fs::read_to_string(report.path()).unwrap();
    assert!(
        xml.contains(&format!("tests=\"{expected}\"")),
        "{class_name} did not run exactly {expected} tests: {xml}"
    );
    assert!(xml.contains("failures=\"0\""), "{class_name} failed: {xml}");
    assert!(xml.contains("errors=\"0\""), "{class_name} errored: {xml}");
    assert!(
        xml.contains("skipped=\"0\""),
        "{class_name} skipped a test: {xml}"
    );
}

/// Generate each proof application exactly once per `cargo test` process.
/// Compilation and execution happen in `verified_app_fixtures`: Maven's
/// `verify` lifecycle already includes compile and test-compile, so running a
/// separate `test-compile` lifecycle first repeated three Maven startups while
/// proving a strict subset of the same result.
fn generated_app_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static GENERATED: std::sync::OnceLock<Vec<(&'static str, std::path::PathBuf)>> =
        std::sync::OnceLock::new();
    GENERATED.get_or_init(|| {
        let cache_salt = SPRING_APP_MANIFESTS.iter().fold(
            PROOF_APP_CACHE_SCHEMA.to_string(),
            |mut salt, (name, manifest)| {
                salt.push_str(&format!("\n{name}:{}\n{manifest}", manifest.len()));
                salt
            },
        );
        let (parent, fresh) = cached_toolchain_dir_with_salt("proof-apps", &cache_salt);
        if !fresh {
            let generated: Vec<_> = SPRING_APP_MANIFESTS
                .iter()
                .map(|(name, _)| (*name, parent.join(name)))
                .collect();
            for (_, root) in &generated {
                validate_proof_app_shared_context(root);
            }
            return generated;
        }
        let mut generated = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = SPRING_APP_MANIFESTS
                .iter()
                .map(|&(name, manifest)| {
                    let parent = &parent;
                    scope.spawn(move || {
                        let root = parent.join(name);
                        fs::create_dir_all(&root).unwrap();
                        write_spring_fixture(&root);
                        fs::create_dir_all(root.join(".jails")).unwrap();
                        fs::write(root.join(".jails/app.toml"), manifest).unwrap();

                        let output = jails_cmd_with_path(&root, path)
                            .args(["app", "apply", "--no-start"])
                            .output()
                            .unwrap();
                        assert!(
                            output.status.success(),
                            "{name} apply: stdout={} stderr={}",
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr)
                        );
                        add_app_unit_test_database(&root);
                        (name, root)
                    })
                })
                .collect();
            for handle in handles {
                generated.push(handle.join().unwrap());
            }
        });
        for (_, root) in &generated {
            align_proof_app_smoke_context(root);
        }
        mark_toolchain_dir_generated(&parent);
        generated
    })
}

fn add_app_unit_test_database(root: &Path) {
    const H2: &str = r#"        <dependency>
            <groupId>com.h2database</groupId>
            <artifactId>h2</artifactId>
            <scope>test</scope>
        </dependency>
"#;
    let pom_path = root.join("pom.xml");
    let pom = fs::read_to_string(&pom_path).unwrap();
    let marker = "    </dependencies>\n";
    assert!(
        pom.contains(marker),
        "generated app POM has no dependencies"
    );
    fs::write(pom_path, pom.replacen(marker, &format!("{H2}{marker}"), 1)).unwrap();
}

fn align_proof_app_smoke_context(project: &Path) {
    const BARE: &str = "@SpringBootTest\nclass DemoApplicationTests";

    for (file, class_name) in [
        ("ActuatorEndpointsTest.java", "ActuatorEndpointsTest"),
        ("PrometheusScrapeTest.java", "PrometheusScrapeTest"),
    ] {
        assert_proof_app_context_source(project, file, class_name);
    }

    let test = project.join("src/test/java/com/example/demo/DemoApplicationTests.java");
    let source = fs::read_to_string(&test).unwrap();
    let shared = format!("{PROOF_APP_SHARED_SPRING_BOOT_TEST}\nclass DemoApplicationTests");
    if !source.contains(&shared) {
        assert!(
            source.contains(BARE),
            "proof-app smoke test no longer has the expected Spring context annotation: {source}"
        );
        fs::write(&test, source.replacen(BARE, &shared, 1)).unwrap();
    }
    assert_proof_app_context_source(project, "DemoApplicationTests.java", "DemoApplicationTests");
}

const PROOF_APP_SHARED_SPRING_BOOT_TEST: &str = r#"@SpringBootTest(
        webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT,
        properties = {
            "management.server.port=0",
            "app.security.dev.username=prometheus-probe",
            "app.security.dev.password=prometheus-probe"
        })"#;

fn validate_proof_app_shared_context(project: &Path) {
    for (file, class_name) in [
        ("DemoApplicationTests.java", "DemoApplicationTests"),
        ("ActuatorEndpointsTest.java", "ActuatorEndpointsTest"),
        ("PrometheusScrapeTest.java", "PrometheusScrapeTest"),
    ] {
        assert_proof_app_context_source(project, file, class_name);
    }
}

fn assert_proof_app_context_source(project: &Path, file: &str, class_name: &str) {
    let test_dir = project.join("src/test/java/com/example/demo");
    let source = fs::read_to_string(test_dir.join(file)).unwrap();
    let class_marker = format!("class {class_name}");
    let class_start = source
        .find(&class_marker)
        .unwrap_or_else(|| panic!("{file} has no {class_marker}: {source}"));
    let annotations = &source[..class_start];
    let context_start = annotations
        .rfind("@Import(")
        .unwrap_or_else(|| panic!("{file} has no context import: {source}"));
    assert_eq!(
        annotations[context_start..].trim_end(),
        format!("@Import(TestcontainersConfig.class)\n{PROOF_APP_SHARED_SPRING_BOOT_TEST}"),
        "{file} drifted from the proof application's shared Spring context"
    );
}

fn verified_app_unit_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let generated = generated_app_fixtures(path);
    VERIFIED.get_or_init(|| {
        std::thread::scope(|scope| {
            for (name, root) in generated {
                scope.spawn(move || {
                    let mut command = real_maven_cmd(root, path);
                    configure_app_unit_maven(&mut command, name);
                    let status = command.args(["-q", "test"]).status().unwrap();
                    assert!(status.success(), "{name} failed its Surefire tests");
                });
            }
        });
    });
    generated
}

fn verified_app_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let suite_started = std::time::Instant::now();
        let profile_stage = |stage: &str| {
            if std::env::var_os("JAILS_TEST_PROFILE").is_some() {
                eprintln!(
                    "JAILS_TEST_PROFILE app_stage={stage} elapsed_ms={}",
                    suite_started.elapsed().as_millis()
                );
            }
        };
        let names = SPRING_APP_MANIFESTS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        let (services_launched, launch_ready) = std::sync::mpsc::channel();
        let (postgres_ready, wait_for_postgres) = std::sync::mpsc::channel();
        let service_start = std::thread::spawn(move || {
            AppSuiteServices::start(&names, services_launched, postgres_ready)
        });
        let generated = generated_app_fixtures(path);
        profile_stage("fixtures-ready");
        let endpoints = launch_ready.recv().unwrap();
        profile_stage("containers-launched");
        let image_build = std::thread::spawn(|| verified_app_images(generated));

        // Compile and execute Surefire while PostgreSQL/Kafka are starting.
        // Failsafe follows once both real services are ready, so every test
        // still runs exactly once without a skip flag or selector.
        verified_app_unit_fixtures(path);
        profile_stage("surefire-complete");
        wait_for_postgres.recv().unwrap();
        let services = service_start.join().unwrap();
        profile_stage("services-ready");
        std::thread::scope(|scope| {
            for (name, root) in generated {
                scope.spawn(move || {
                    let mut command = real_maven_cmd(root, path);
                    endpoints.configure_maven(&mut command, name);
                    let status = command
                        .args(["-q", "failsafe:integration-test", "failsafe:verify"])
                        .status()
                        .unwrap();
                    assert!(
                        status.success(),
                        "{name} failed its Failsafe integration tests"
                    );
                });
            }
        });
        profile_stage("failsafe-complete");
        let mut reports = MavenReportSummary::default();
        for (_, root) in generated {
            reports.add(maven_report_summary(root, "failsafe-reports"));
        }
        assert_eq!(
            reports,
            MavenReportSummary {
                // 70 -> 72: `aSinkThatAlreadyAcceptedIsNotSentTheEventAgain`,
                // one per outbox, proving the relay's per-sink record survives
                // a failed attempt (plan.md P6.3).
                reports: 47,
                tests: 72,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the proof applications must execute every Failsafe test"
        );
        drop(services);
        profile_stage("services-stopped");
        image_build.join().unwrap();
        profile_stage("images-complete");
    });
    generated_app_fixtures(path)
}

fn verified_app_images(fixtures: &'static Vec<(&'static str, std::path::PathBuf)>) {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        // Podman does not single-flight concurrent pulls: three `docker build
        // --pull` calls downloaded the same Maven and Temurin layers three times,
        // consuming a gigabyte of memory without increasing coverage. Resolve
        // every generated FROM image once, then let the still-parallel builds use
        // the local content-addressed image store.
        let mut base_images = std::collections::BTreeSet::new();
        for (_, root) in fixtures {
            let dockerfile = fs::read_to_string(root.join("Dockerfile")).unwrap();
            for line in dockerfile.lines().filter(|line| line.starts_with("FROM ")) {
                if let Some(image) = line.split_whitespace().nth(1) {
                    base_images.insert(image.to_string());
                }
            }
        }
        for image in base_images {
            let present = std::process::Command::new("docker")
                .args(["image", "inspect", &image])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !present {
                let status = real_docker_cmd(Path::new("."))
                    .args(["pull", &image])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "could not pull generated base image {image}"
                );
            }
        }
        // Podman serialises parts of its rootless storage graph internally.
        // Three client processes made three cache-only builds take about six
        // seconds each, versus roughly 1.2 seconds apiece in sequence. This
        // loop still builds and inspects every image, while the whole image
        // phase remains overlapped with the Maven application gate.
        for (name, root) in fixtures {
            let image = format!("jails-dogfood-{name}:test");
            let status = real_docker_cmd(root)
                // Required FROM images were inspected/pulled above. Podman's
                // default `--pull=missing` can still wait for registry
                // resolution before accepting its local copy; make this
                // deliberately cached build local-only.
                //
                // **`--pull=false`, not `--pull=never`.** `never` is podman's
                // spelling of this policy and real Docker rejects it outright
                // -- `invalid argument "never" for "--pull" flag`, before the
                // build starts -- so on Docker this gate could only ever fail,
                // and it failed with `web-crawler failed its generated OCI
                // image build`, which reads like the generated Dockerfile is
                // wrong. Podman accepts the boolean spelling as an alias for
                // `never`, so `false` is the one word both engines understand.
                // The same trap as the `docker info --format` one CLAUDE.md
                // records, in the other direction: this machine's `docker` is
                // podman's shim, so a podman-only flag looks portable here.
                .args(["build", "--pull=false", "--tag", &image, "."])
                .status()
                .unwrap();
            assert!(
                status.success(),
                "{name} failed its generated OCI image build"
            );
            let inspect = std::process::Command::new("docker")
                .args(["image", "inspect", &image, "--format", "{{.Config.User}}"])
                .output()
                .unwrap();
            assert!(inspect.status.success(), "could not inspect {image}");
            assert_eq!(
                String::from_utf8_lossy(&inspect.stdout).trim(),
                "10001:10001",
                "{name} image did not retain the non-root runtime user"
            );
        }
    });
}

/// A minimal project skeleton (pom.xml + an *Application.java) good enough
/// for generate/destroy's path resolution -- not a real, resolvable Maven
/// project, since these tests never invoke Maven.
fn write_project_skeleton(root: &std::path::Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\n\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

// ---- mocked mvn/mvnd: verify jails' own command-construction logic
// (which binary, which args) without needing real Maven. ----

// ---- real Maven + JDK 26, no network beyond Maven Central artifact
// resolution (never start.spring.io): verify the actual bar the tool
// exists for -- "does new-cli produce a project that passes mvn test?" and
// "does generate scaffold produce a project that compiles?". Skipped
// automatically if mvn isn't on PATH. ----

// ---- add ----

fn write_release_fixture(root: &std::path::Path, release: &str) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        root.join("pom.xml"),
        format!("<project>\n    <properties>\n        <maven.compiler.release>{release}</maven.compiler.release>\n    </properties>\n    <dependencies>\n    </dependencies>\n</project>\n"),
    )
    .unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

// ---- observation and refactoring commands (doctor / routes / beans /
// rename / why). All offline: they read source and configuration, never
// Maven. ----

/// A minimal Spring-shaped project: an application class, a controller, a
/// service, and a repository interface with an implementation. Enough for
/// `routes`, `beans` and `rename` to have something real to say.
fn write_inspectable_project(root: &Path) {
    fs::write(
        root.join("pom.xml"),
        "<project><parent><groupId>org.springframework.boot</groupId>\
         <artifactId>spring-boot-starter-parent</artifactId></parent>\
         <artifactId>shop</artifactId>\
         <properties><maven.compiler.release>27</maven.compiler.release></properties></project>",
    )
    .unwrap();
    let main = root.join("src/main/java/dev/example/shop");
    fs::create_dir_all(main.join("api")).unwrap();
    fs::create_dir_all(main.join("domain")).unwrap();
    fs::write(
        main.join("ShopApplication.java"),
        "package dev.example.shop;\npublic class ShopApplication {}\n",
    )
    .unwrap();
    fs::write(
        main.join("api/OrderController.java"),
        "package dev.example.shop.api;\n\
         @RestController\n\
         @RequestMapping(\"/orders\")\n\
         public final class OrderController {\n\
         \x20   public OrderController(OrderService service) {}\n\
         \x20   @GetMapping(\"/{id}\")\n\
         \x20   public Order byId(String id) { return null; }\n\
         \x20   @PostMapping\n\
         \x20   public Order create(Order order) { return null; }\n\
         }\n",
    )
    .unwrap();
    fs::write(
        main.join("domain/Order.java"),
        "package dev.example.shop.domain;\npublic record Order(String id) {}\n",
    )
    .unwrap();
    fs::write(
        main.join("domain/OrderService.java"),
        "package dev.example.shop.domain;\n\
         @Service\n\
         public final class OrderService {\n\
         \x20   public OrderService(Order seed) {}\n\
         }\n",
    )
    .unwrap();
}

// ---- Spring-only capabilities. The generated code targets Spring Boot 4 /
// Framework 7 APIs, so the only honest check is a real compile. ----

// ---- the manifest: `jails.toml` describes the project, `sync` makes it true ----
