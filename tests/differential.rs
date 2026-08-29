//! Differential canaries for the compiler cutover.
//!
//! The ordinary test run compares the still-live legacy route with the
//! canonical JDL route in the current binary. `JAILS_LEGACY_BIN` replaces the
//! legacy side with a binary built from the frozen pre-cutover revision, which
//! is how the same scenarios keep working after the legacy crates are deleted.

mod common;

use common::{
    Adopted, adopted_base, adopted_reader_bytes, temp_dir, write_adopted_fixture,
    write_plain_fixture, write_spring_fixture,
};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EMPTY_JDL: &str = "application Demo @id(project_demo)\n\
package com.example.demo\n\
java 26\n\
dialect postgresql\n";

struct Subject {
    name: &'static str,
    binary: OsString,
    root: PathBuf,
    record: PathBuf,
}

impl Subject {
    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(&self.binary)
            .current_dir(&self.root)
            .args(arguments)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "could not run {} binary `{}`: {error}",
                    self.name,
                    Path::new(&self.binary).display()
                )
            })
    }

    fn succeeds(&self, arguments: &[&str]) {
        let output = self.run(arguments);
        assert!(
            output.status.success(),
            "{} `jails {}` failed:\n{}{}",
            self.name,
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn subjects(label: &str) -> [Subject; 2] {
    subjects_with_fixture(label, false)
}

fn spring_subjects(label: &str) -> [Subject; 2] {
    subjects_with_fixture(label, true)
}

fn subjects_with_fixture(label: &str, spring: bool) -> [Subject; 2] {
    let legacy_root = temp_dir(&format!("differential-{label}-legacy"));
    let canonical_root = temp_dir(&format!("differential-{label}-canonical"));
    if spring {
        write_spring_fixture(&legacy_root);
        write_spring_fixture(&canonical_root);
    } else {
        write_plain_fixture(&legacy_root);
        write_plain_fixture(&canonical_root);
    }
    fs::create_dir_all(canonical_root.join(".jails")).unwrap();
    fs::write(canonical_root.join(".jails/model.jdl"), EMPTY_JDL).unwrap();

    [
        Subject {
            name: "legacy",
            binary: std::env::var_os("JAILS_LEGACY_BIN")
                .unwrap_or_else(|| OsString::from(env!("CARGO_BIN_EXE_jails"))),
            record: legacy_root.join("src/main/java/com/example/demo/domain/Task.java"),
            root: legacy_root,
        },
        Subject {
            name: "canonical",
            binary: OsString::from(env!("CARGO_BIN_EXE_jails")),
            record: canonical_root
                .join(".jails/generated/main/java/com/example/demo/domain/Task.java"),
            root: canonical_root,
        },
    ]
}

/// This is the product loop, run through both implementations. It deliberately
/// compares behavior rather than private state bytes: the legacy object store
/// and the canonical merge bases have different encodings and neither is a
/// user-visible compatibility contract.
#[test]
fn generate_edit_generate_has_the_same_safety_contract_on_both_implementations() {
    let subjects = subjects("iterative-record");
    for subject in &subjects {
        subject.succeeds(&["g", "record", "Task", "title:string!"]);
        let source = fs::read_to_string(&subject.record).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            &subject.record,
            format!(
                "{}\n\n    public String handWritten() {{ return title; }}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();

        subject.succeeds(&["g", "field", "Task", "done:boolean"]);
        let source = fs::read_to_string(&subject.record).unwrap();
        assert!(
            source.contains("handWritten()"),
            "{}: {source}",
            subject.name
        );
        assert!(
            source.contains("boolean done"),
            "{} changed the required primitive ABI: {source}",
            subject.name
        );

        let edited = source.replace("title must not be blank", "give me a useful title");
        assert_ne!(edited, source, "{} has no validation message", subject.name);
        fs::write(&subject.record, edited).unwrap();
        subject.succeeds(&["g", "field", "Task", "priority:int"]);
        let source = fs::read_to_string(&subject.record).unwrap();
        assert!(
            source.contains("give me a useful title"),
            "{}: {source}",
            subject.name
        );
        assert!(
            source.contains("int priority"),
            "{} changed the required primitive ABI: {source}",
            subject.name
        );

        fs::write(
            &subject.record,
            source.replace("int priority", "long priority"),
        )
        .unwrap();
        let before = snapshot(&subject.root);
        let conflict = subject.run(&["g", "field", "Task", "dueAt:instant"]);
        assert!(
            !conflict.status.success(),
            "{} accepted an overlapping generated-line edit",
            subject.name
        );
        assert!(
            String::from_utf8_lossy(&conflict.stderr).contains("overlap"),
            "{}: {}",
            subject.name,
            String::from_utf8_lossy(&conflict.stderr)
        );
        assert_eq!(
            snapshot(&subject.root),
            before,
            "{} wrote files after refusing the conflict",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn source_units_keep_reader_edits_on_every_emitted_java_file() {
    let plain = subjects("iterative-source-units");
    for subject in &plain {
        subject.succeeds(&["g", "class", "Clock"]);
        subject.succeeds(&["g", "interface", "Port"]);
        subject.succeeds(&["g", "test", "ParserTest"]);
        subject.succeeds(&["g", "integration-test", "CheckoutIT"]);
        let files = [
            source_unit_path(subject, "main", "", "Clock.java"),
            source_unit_path(subject, "test", "", "ClockTest.java"),
            source_unit_path(subject, "main", "", "Port.java"),
            source_unit_path(subject, "test", "", "ParserTest.java"),
            source_unit_path(subject, "test", "", "CheckoutIT.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("plain-reader-edit-{index}"));
        }
        subject.succeeds(&["g", "class", "Queue"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("plain-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }
        let pom = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        assert!(
            pom.contains("maven-failsafe-plugin"),
            "{} did not wire its integration test: {pom}",
            subject.name
        );
        assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    }

    let spring = spring_subjects("iterative-service-unit");
    for subject in &spring {
        subject.succeeds(&["g", "service", "BillingService"]);
        let files = [
            source_unit_path(subject, "main", "service", "BillingService.java"),
            source_unit_path(subject, "test", "service", "BillingServiceTest.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("service-reader-edit-{index}"));
        }
        subject.succeeds(&["g", "service", "Shipping"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("service-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }
    }

    for subject in plain.into_iter().chain(spring) {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn sealed_evolution_preserves_edits_in_the_type_and_exhaustive_test() {
    let subjects = subjects("sealed-source-unit");
    for subject in &subjects {
        subject.succeeds(&["g", "sealed", "Outcome", "Accepted", "Rejected"]);
        let files = [
            source_unit_path(subject, "main", "domain", "Outcome.java"),
            source_unit_path(subject, "test", "domain", "OutcomeTest.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            let anchor = if index == 0 {
                "public sealed interface Outcome permits Outcome.Accepted, Outcome.Rejected {\n"
            } else {
                "class OutcomeTest {\n"
            };
            assert!(source.contains(anchor), "{}: {source}", path.display());
            fs::write(
                path,
                source.replace(
                    anchor,
                    &format!("{anchor}\n    // sealed-reader-edit-{index}\n"),
                ),
            )
            .unwrap();
        }

        subject.succeeds(&["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"]);
        for (index, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            assert!(
                source.contains(&format!("sealed-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
            assert!(
                source.contains("Pending"),
                "{} did not evolve {}: {source}",
                subject.name,
                path.display()
            );
        }
        let first = reader_snapshot(&subject.root);
        subject.succeeds(&["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"]);
        assert_eq!(
            reader_snapshot(&subject.root),
            first,
            "{} changed reader-visible state on an identical sealed rerun",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn strategy_variants_preserve_each_implementation_boundary() {
    let subjects = spring_subjects("strategy-source-unit");
    for subject in &subjects {
        subject.succeeds(&["g", "record", "Post", "title:string!"]);
        subject.succeeds(&[
            "g", "strategy", "PostRule", "Featured", "Standard", "--on", "Post",
        ]);
        let root = if subject.name == "canonical" {
            subject.root.join(".jails/generated")
        } else {
            subject.root.join("src")
        };
        let existing = [
            root.join("main/java/com/example/demo/domain/PostRule.java"),
            root.join("main/java/com/example/demo/service/PostRuleEvaluator.java"),
            root.join("main/java/com/example/demo/service/FeaturedPostRule.java"),
            root.join("main/java/com/example/demo/service/StandardPostRule.java"),
            root.join("test/java/com/example/demo/service/FeaturedPostRuleTest.java"),
            root.join("test/java/com/example/demo/service/StandardPostRuleTest.java"),
        ];
        for (index, path) in existing.iter().enumerate() {
            add_reader_comment(path, &format!("strategy-reader-edit-{index}"));
        }

        subject.succeeds(&[
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Post",
        ]);
        for (index, path) in existing.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("strategy-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }
        let premium = [
            root.join("main/java/com/example/demo/service/PremiumPostRule.java"),
            root.join("test/java/com/example/demo/service/PremiumPostRuleTest.java"),
        ];
        assert!(premium.iter().all(|path| path.is_file()));
        let stable = reader_snapshot(&subject.root);
        subject.succeeds(&[
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Post",
        ]);
        assert_eq!(
            reader_snapshot(&subject.root),
            stable,
            "{} changed reader bytes on an identical strategy rerun",
            subject.name
        );

        for (index, path) in existing.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            fs::write(
                path,
                source.replace(&format!("\n\n    // strategy-reader-edit-{index}"), ""),
            )
            .unwrap();
        }
        subject.succeeds(&["destroy", "strategy", "PostRule", "--force"]);
        assert!(existing.iter().chain(&premium).all(|path| !path.exists()));
        assert!(
            subject.record.with_file_name("Post.java").is_file(),
            "{} strategy destroy removed its input record",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn controller_and_test_keep_reader_edits_across_the_product_loop() {
    let subjects = spring_subjects("controller-source-unit");
    for subject in &subjects {
        subject.succeeds(&["g", "controller", "Verify", "--path", "/verify"]);
        let files = [
            source_unit_path(subject, "main", "web", "VerifyController.java"),
            source_unit_path(subject, "test", "web", "VerifyControllerTest.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("controller-reader-edit-{index}"));
        }

        // A later generation exercises replay/reconciliation of the already
        // edited controller without relying on either implementation's state
        // encoding.
        subject.succeeds(&["g", "class", "Trigger"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("controller-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }
        let stable = files
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>();
        subject.succeeds(&["g", "controller", "Verify", "--path", "/verify"]);
        for (index, path) in files.iter().enumerate() {
            assert_eq!(
                fs::read(path).unwrap(),
                stable[index],
                "{} changed {} on an identical controller rerun",
                subject.name,
                path.display()
            );
        }

        for (index, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            fs::write(
                path,
                source.replace(&format!("\n\n    // controller-reader-edit-{index}"), ""),
            )
            .unwrap();
        }
        subject.succeeds(&["destroy", "controller", "Verify", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn data_capability_packs_preserve_named_package_edits_and_remove_symmetrically() {
    let subjects = subjects("data-capability-packs");
    for subject in &subjects {
        subject.succeeds(&["add", "json", "--name", "Dataset"]);
        let root = if subject.name == "canonical" {
            subject.root.join(".jails/generated")
        } else {
            subject.root.join("src")
        };
        let files = [
            root.join("main/java/com/example/demo/adapters/DatasetJson.java"),
            root.join("test/java/com/example/demo/adapters/DatasetJsonTest.java"),
        ];
        assert!(files.iter().all(|path| path.is_file()));
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("json-pack-reader-edit-{index}"));
        }

        subject.succeeds(&["add", "csv"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("json-pack-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }
        let stable = files
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>();
        subject.succeeds(&["add", "json", "--name", "Dataset"]);
        assert_eq!(
            files
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            stable,
            "{} changed data-pack bytes on an identical rerun",
            subject.name
        );

        for (index, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            fs::write(
                path,
                source.replace(&format!("\n\n    // json-pack-reader-edit-{index}"), ""),
            )
            .unwrap();
        }
        subject.succeeds(&["remove", "json", "--name", "Dataset", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        assert!(
            root.join("main/java/com/example/demo/adapters/CsvReader.java")
                .is_file(),
            "{} removed the independent CSV pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn http_and_fake_packs_preserve_edits_and_remove_as_whole_boundaries() {
    let subjects = subjects("http-fake-capability-packs");
    for subject in &subjects {
        subject.succeeds(&["add", "fake"]);
        subject.succeeds(&["add", "http", "--name", "Admin"]);
        let files = [
            source_unit_path(subject, "test", "testkit", "Fake.java"),
            source_unit_path(subject, "test", "testkit", "FakeTest.java"),
            source_unit_path(subject, "main", "api", "AdminServer.java"),
            source_unit_path(subject, "test", "api", "AdminServerTest.java"),
        ];
        assert!(files.iter().all(|path| path.is_file()));
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("tool-pack-reader-edit-{index}"));
        }

        subject.succeeds(&["add", "csv"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("tool-pack-reader-edit-{index}")),
                "{} lost reader edit in {}",
                subject.name,
                path.display()
            );
        }

        for (index, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            fs::write(
                path,
                source.replace(&format!("\n\n    // tool-pack-reader-edit-{index}"), ""),
            )
            .unwrap();
        }
        subject.succeeds(&["remove", "http", "--name", "Admin", "--force"]);
        assert!(files[2..].iter().all(|path| !path.exists()));
        assert!(files[..2].iter().all(|path| path.exists()));
        subject.succeeds(&["remove", "fake", "--force"]);
        assert!(files[..2].iter().all(|path| !path.exists()));
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn testkit_merges_java_and_resource_edits_and_removes_them_together() {
    let subjects = subjects("testkit-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "testkit"]);
        let java = source_unit_path(subject, "test", "testkit", "Clocks.java");
        let resource_root = if subject.name == "canonical" {
            subject.root.join(".jails/generated/test/resources")
        } else {
            subject.root.join("src/test/resources")
        };
        let fixture = resource_root.join("fixtures/example.json");
        add_reader_comment(&java, "testkit-reader-edit");
        let clean_fixture = fs::read_to_string(&fixture).unwrap();
        fs::write(&fixture, clean_fixture.replace("bolt", "reader-bolt")).unwrap();

        subject.succeeds(&["add", "fake"]);
        assert!(
            fs::read_to_string(&java)
                .unwrap()
                .contains("testkit-reader-edit"),
            "{} lost the Java edit",
            subject.name
        );
        assert!(
            fs::read_to_string(&fixture)
                .unwrap()
                .contains("reader-bolt"),
            "{} lost the resource edit",
            subject.name
        );

        let clean_java = fs::read_to_string(&java)
            .unwrap()
            .replace("\n\n    // testkit-reader-edit", "");
        fs::write(&java, clean_java).unwrap();
        fs::write(&fixture, clean_fixture).unwrap();
        subject.succeeds(&["remove", "testkit", "--force"]);
        assert!(!java.exists());
        assert!(!fixture.exists());
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn sqlite_pack_merges_all_java_and_migration_resource_edits() {
    let subjects = subjects("sqlite-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "sqlite", "--name", "Store"]);
        let files = [
            source_unit_path(subject, "main", "adapters", "StoreDatabase.java"),
            source_unit_path(subject, "main", "adapters", "StoreMigrations.java"),
            source_unit_path(subject, "test", "adapters", "StoreDatabaseTest.java"),
        ];
        let migration = if subject.name == "canonical" {
            subject
                .root
                .join("src/main/resources/db/migration/V001__sqlite_init.sql")
        } else {
            subject
                .root
                .join("src/main/resources/db/migration/001_init.sql")
        };
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("sqlite-reader-edit-{index}"));
        }
        let clean_migration = fs::read_to_string(&migration).unwrap();
        fs::write(
            &migration,
            clean_migration.replace("Applied once", "Reader wording survives; applied once"),
        )
        .unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("sqlite-reader-edit-{index}")),
                "{} lost {}",
                subject.name,
                path.display()
            );
        }
        assert!(
            fs::read_to_string(&migration)
                .unwrap()
                .contains("Reader wording survives"),
            "{} lost the migration resource edit",
            subject.name
        );

        for (index, path) in files.iter().enumerate() {
            let clean = fs::read_to_string(path)
                .unwrap()
                .replace(&format!("\n\n    // sqlite-reader-edit-{index}"), "");
            fs::write(path, clean).unwrap();
        }
        subject.succeeds(&["remove", "sqlite", "--name", "Store", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        assert!(
            migration.exists(),
            "{} deleted migration history",
            subject.name
        );
        assert!(
            fs::read_to_string(&migration)
                .unwrap()
                .contains("Reader wording survives"),
            "{} lost the historical migration edit",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn h2_pack_preserves_java_and_reader_properties_then_removes_only_owned_state() {
    let subjects = spring_subjects("h2-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "h2"]);
        let java = source_unit_path(subject, "test", "adapters", "H2DatabaseTest.java");
        add_reader_comment(&java, "h2-reader-edit");
        let main_properties = subject
            .root
            .join("src/main/resources/application.properties");
        let test_properties = subject
            .root
            .join("src/test/resources/config/application.properties");
        for (path, line) in [
            (&main_properties, "reader.h2.main=survives\n"),
            (&test_properties, "reader.h2.test=survives\n"),
        ] {
            let mut source = fs::read_to_string(path).unwrap_or_default();
            if !source.ends_with('\n') && !source.is_empty() {
                source.push('\n');
            }
            source.push_str(line);
            fs::write(path, source).unwrap();
        }

        subject.succeeds(&["add", "fake"]);
        assert!(
            fs::read_to_string(&java)
                .unwrap()
                .contains("h2-reader-edit"),
            "{} lost the H2 Java edit",
            subject.name
        );
        for (path, reader_key, h2_value) in [
            (
                &main_properties,
                "reader.h2.main",
                "jdbc:h2:file:./data/app",
            ),
            (&test_properties, "reader.h2.test", "jdbc:h2:mem:test"),
        ] {
            let source = fs::read_to_string(path).unwrap();
            assert!(
                source.contains(reader_key),
                "{} lost {reader_key}",
                subject.name
            );
            assert!(
                source.contains(h2_value),
                "{} lost {h2_value}",
                subject.name
            );
        }

        let clean = fs::read_to_string(&java)
            .unwrap()
            .replace("\n\n    // h2-reader-edit", "");
        fs::write(&java, clean).unwrap();
        subject.succeeds(&["remove", "h2", "--force"]);
        assert!(!java.exists());
        for (path, reader_key) in [
            (&main_properties, "reader.h2.main"),
            (&test_properties, "reader.h2.test"),
        ] {
            let source = fs::read_to_string(path).unwrap();
            assert!(
                source.contains(reader_key),
                "{} removed {reader_key}",
                subject.name
            );
            assert!(
                !source.contains("jdbc:h2:"),
                "{} retained H2 config",
                subject.name
            );
        }
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn actuator_pack_preserves_java_and_reader_properties_then_removes_only_owned_state() {
    let subjects = spring_subjects("actuator-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "actuator"]);
        let java = source_unit_path(subject, "test", "", "ActuatorEndpointsTest.java");
        add_reader_comment(&java, "actuator-reader-edit");
        let properties = subject
            .root
            .join("src/main/resources/application.properties");
        let mut source = fs::read_to_string(&properties).unwrap_or_default();
        if !source.ends_with('\n') && !source.is_empty() {
            source.push('\n');
        }
        source.push_str("reader.actuator=survives\n");
        fs::write(&properties, source).unwrap();

        subject.succeeds(&["add", "fake"]);
        assert!(
            fs::read_to_string(&java)
                .unwrap()
                .contains("actuator-reader-edit"),
            "{} lost the Actuator Java edit",
            subject.name
        );
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.actuator=survives"),
            "{} lost the reader property",
            subject.name
        );
        assert!(
            source.contains(
                "management.endpoints.web.exposure.include=health,info,prometheus,threaddump"
            ),
            "{} lost the Actuator exposure contract",
            subject.name
        );
        assert!(
            !source.contains("management.endpoints.web.exposure.include=*"),
            "{} exposed every Actuator endpoint",
            subject.name
        );

        let clean = fs::read_to_string(&java)
            .unwrap()
            .replace("\n\n    // actuator-reader-edit", "");
        fs::write(&java, clean).unwrap();
        subject.succeeds(&["remove", "actuator", "--force"]);
        assert!(!java.exists());
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.actuator=survives"),
            "{} removed the reader property",
            subject.name
        );
        for owned in [
            "management.server.port=8081",
            "management.endpoints.web.base-path=/management",
            "management.endpoint.health.group.liveness.include=ping",
            "info.app.name=@project.name@",
        ] {
            assert!(
                !source.contains(owned),
                "{} retained owned Actuator property {owned}",
                subject.name
            );
        }
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn cache_pack_preserves_both_java_files_and_reader_properties_then_removes_owned_state() {
    let subjects = spring_subjects("cache-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "cache"]);
        let files = [
            source_unit_path(subject, "main", "", "CacheConfig.java"),
            source_unit_path(subject, "test", "", "CacheConfigTest.java"),
        ];
        let originals = files
            .each_ref()
            .map(|path| fs::read_to_string(path).unwrap());
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("cache-reader-edit-{index}"));
        }
        let properties = subject
            .root
            .join("src/main/resources/application.properties");
        let mut source = fs::read_to_string(&properties).unwrap_or_default();
        if !source.ends_with('\n') && !source.is_empty() {
            source.push('\n');
        }
        source.push_str("reader.cache=survives\n");
        fs::write(&properties, source).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("cache-reader-edit-{index}")),
                "{} lost the cache edit in {}",
                subject.name,
                path.display()
            );
        }
        let source = fs::read_to_string(&properties).unwrap();
        for required in [
            "reader.cache=survives",
            "spring.cache.type=caffeine",
            "spring.cache.caffeine.spec=maximumSize=1000,expireAfterWrite=60s",
        ] {
            assert!(
                source.contains(required),
                "{} lost {required}",
                subject.name
            );
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "cache", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.cache=survives"),
            "{} removed the reader property",
            subject.name
        );
        assert!(
            !source.contains("spring.cache."),
            "{} retained cache configuration",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn cors_pack_preserves_both_java_files_and_reader_properties_then_removes_owned_state() {
    let subjects = spring_subjects("cors-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "cors"]);
        let files = [
            source_unit_path(subject, "main", "", "CorsConfig.java"),
            source_unit_path(subject, "test", "", "CorsConfigTest.java"),
        ];
        let originals = files
            .each_ref()
            .map(|path| fs::read_to_string(path).unwrap());
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("cors-reader-edit-{index}"));
        }
        let properties = subject
            .root
            .join("src/main/resources/application.properties");
        let mut source = fs::read_to_string(&properties).unwrap_or_default();
        if !source.ends_with('\n') && !source.is_empty() {
            source.push('\n');
        }
        source.push_str("reader.cors=survives\n");
        fs::write(&properties, source).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("cors-reader-edit-{index}")),
                "{} lost the CORS edit in {}",
                subject.name,
                path.display()
            );
        }
        let source = fs::read_to_string(&properties).unwrap();
        for required in [
            "reader.cors=survives",
            "app.cors.allowed-origins=https://example.invalid",
        ] {
            assert!(
                source.contains(required),
                "{} lost {required}",
                subject.name
            );
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "cors", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.cors=survives"),
            "{} removed the reader property",
            subject.name
        );
        assert!(
            !source.contains("app.cors.allowed-origins"),
            "{} retained CORS configuration",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn observability_pack_preserves_all_java_files_then_removes_only_owned_state() {
    let subjects = spring_subjects("observability-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "observability"]);
        let files = [
            source_unit_path(subject, "main", "", "MetricsConfig.java"),
            source_unit_path(subject, "main", "", "AppMetrics.java"),
            source_unit_path(subject, "test", "", "AppMetricsTest.java"),
            source_unit_path(subject, "test", "", "PrometheusScrapeTest.java"),
        ];
        let originals = files
            .each_ref()
            .map(|path| fs::read_to_string(path).unwrap());
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("observability-reader-edit-{index}"));
        }
        let properties = subject
            .root
            .join("src/main/resources/application.properties");
        let mut source = fs::read_to_string(&properties).unwrap_or_default();
        if !source.ends_with('\n') && !source.is_empty() {
            source.push('\n');
        }
        source.push_str("reader.observability=survives\n");
        fs::write(&properties, source).unwrap();

        subject.succeeds(&["add", "fake"]);
        subject.succeeds(&["add", "actuator"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("observability-reader-edit-{index}")),
                "{} lost the observability edit in {}",
                subject.name,
                path.display()
            );
        }
        let source = fs::read_to_string(&properties).unwrap();
        for required in [
            "reader.observability=survives",
            "management.endpoints.web.exposure.include=health,info,prometheus,threaddump",
            "management.metrics.distribution.slo.http.server.requests=100ms,250ms,500ms,1s,2s,5s,10s",
            "management.tracing.sampling.probability=0.1",
            "server.tomcat.accesslog.directory=/dev",
        ] {
            assert!(
                source.contains(required),
                "{} lost {required}",
                subject.name
            );
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "observability", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.observability=survives"),
            "{} removed the reader property",
            subject.name
        );
        for removed in [
            "management.metrics.distribution.",
            "management.tracing.",
            "server.tomcat.accesslog.",
        ] {
            assert!(
                !source.contains(removed),
                "{} retained observability configuration {removed}",
                subject.name
            );
        }
        for shared in [
            "management.endpoints.web.exposure.include=health,info,prometheus,threaddump",
            "management.server.port=8081",
            "management.endpoint.health.group.readiness.include=ping",
        ] {
            assert!(
                source.contains(shared),
                "{} removed shared Actuator configuration {shared}",
                subject.name
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        assert!(
            !build.contains("micrometer-registry-prometheus"),
            "{} retained the Prometheus registry",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "", "ActuatorEndpointsTest.java").is_file(),
            "{} removed the shared Actuator capability",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn security_pack_preserves_five_edits_and_removal_keeps_the_cors_boundary() {
    let subjects = spring_subjects("security-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "security"]);
        let files = [
            source_unit_path(subject, "main", "", "SecurityConfig.java"),
            source_unit_path(subject, "main", "", "ProductionSecurityConfig.java"),
            source_unit_path(subject, "main", "", "ScopeAuthorizer.java"),
            source_unit_path(subject, "test", "", "SecurityConfigTest.java"),
            source_unit_path(subject, "test", "", "ScopeAuthorizerTest.java"),
        ];
        let originals = files
            .each_ref()
            .map(|path| fs::read_to_string(path).unwrap());
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("security-reader-edit-{index}"));
        }

        subject.succeeds(&["add", "cors"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("security-reader-edit-{index}")),
                "{} lost the security edit in {}",
                subject.name,
                path.display()
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for artifact in [
            "spring-boot-starter-security",
            "spring-boot-starter-oauth2-resource-server",
            "spring-security-test",
            "spring-boot-starter-webmvc-test",
        ] {
            assert!(build.contains(artifact), "{} lost {artifact}", subject.name);
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "security", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        for path in [
            source_unit_path(subject, "main", "", "CorsConfig.java"),
            source_unit_path(subject, "test", "", "CorsConfigTest.java"),
        ] {
            assert!(
                path.is_file(),
                "{} removed {}",
                subject.name,
                path.display()
            );
        }
        let properties = fs::read_to_string(
            subject
                .root
                .join("src/main/resources/application.properties"),
        )
        .unwrap();
        assert!(properties.contains("app.cors.allowed-origins=https://example.invalid"));
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for removed in [
            "spring-boot-starter-security",
            "spring-boot-starter-oauth2-resource-server",
            "spring-security-test",
        ] {
            assert!(
                !build.contains(removed),
                "{} retained {removed}",
                subject.name
            );
        }
        if subject.name == "canonical" {
            assert!(
                build.contains("spring-boot-starter-webmvc-test"),
                "canonical removed CORS's shared web MVC test starter"
            );
        }
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn sse_pack_preserves_all_four_files_and_only_its_owned_configuration() {
    let subjects = spring_subjects("sse-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "sse"]);
        let files = [
            source_unit_path(subject, "main", "", "EventHub.java"),
            source_unit_path(subject, "main", "", "SchedulingConfig.java"),
            source_unit_path(subject, "main", "web", "EventStreamController.java"),
            source_unit_path(subject, "test", "", "EventHubTest.java"),
        ];
        let originals = files
            .each_ref()
            .map(|path| fs::read_to_string(path).unwrap());
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("sse-reader-edit-{index}"));
        }
        let properties = subject
            .root
            .join("src/main/resources/application.properties");
        let mut source = fs::read_to_string(&properties).unwrap_or_default();
        if !source.ends_with('\n') && !source.is_empty() {
            source.push('\n');
        }
        source.push_str("reader.sse=survives\n");
        fs::write(&properties, source).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("sse-reader-edit-{index}")),
                "{} lost the SSE edit in {}",
                subject.name,
                path.display()
            );
        }
        let source = fs::read_to_string(&properties).unwrap();
        for required in ["reader.sse=survives", "spring.task.scheduling.pool.size=4"] {
            assert!(
                source.contains(required),
                "{} lost {required}",
                subject.name
            );
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "sse", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        let source = fs::read_to_string(&properties).unwrap();
        assert!(
            source.contains("reader.sse=survives"),
            "{} removed the reader property",
            subject.name
        );
        assert!(
            !source.contains("spring.task.scheduling.pool.size"),
            "{} retained SSE scheduling configuration",
            subject.name
        );
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn redis_pack_keeps_java_and_compose_edits_in_the_generate_edit_generate_loop() {
    let subjects = spring_subjects("redis-capability-pack");
    for subject in &subjects {
        // `--no-start`: this case is about the *files* -- the Java the pack
        // writes, the reader edits in them, and the compose block's text --
        // and it reads every one of them off disk. Starting the service was
        // incidental, and it leaked a Redis container and its compose network
        // on every run, which is what eventually exhausted Docker's address
        // pool and took unrelated container tests down with it.
        let added = subject.run(&["add", "redis", "--no-start"]);
        if !added.status.success() {
            let output = format!(
                "{}{}",
                String::from_utf8_lossy(&added.stdout),
                String::from_utf8_lossy(&added.stderr)
            );
            assert_eq!(
                subject.name, "legacy",
                "canonical Redis add failed: {output}"
            );
            assert!(
                output.contains("container engine") && output.contains("written and durable"),
                "legacy Redis add failed before durable publication: {output}"
            );
        }
        let files = [
            source_unit_path(subject, "main", "adapters", "KeyValueStore.java"),
            source_unit_path(subject, "test", "adapters", "KeyValueStoreIT.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("redis-reader-edit-{index}"));
        }

        let compose = subject.root.join("compose.yaml");
        let original_compose = fs::read_to_string(&compose).unwrap();
        assert!(original_compose.contains("image: redis:7-alpine"));
        assert!(!original_compose.contains("redis-data"));
        let edited_compose = original_compose
            .replace(
                "    healthcheck:\n",
                "    restart: unless-stopped\n    healthcheck:\n",
            )
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            );
        fs::write(&compose, edited_compose).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("redis-reader-edit-{index}")),
                "{} lost the Redis edit in {}",
                subject.name,
                path.display()
            );
        }
        let compose_after = fs::read_to_string(&compose).unwrap();
        for reader_edit in [
            "restart: unless-stopped",
            "reader-service:",
            "image: reader:latest",
        ] {
            assert!(
                compose_after.contains(reader_edit),
                "{} lost `{reader_edit}` from compose.yaml: {compose_after}",
                subject.name
            );
        }

        let properties = fs::read_to_string(
            subject
                .root
                .join("src/main/resources/application.properties"),
        )
        .unwrap();
        for property in [
            "spring.data.redis.host=localhost",
            "spring.data.redis.port=6379",
            "app.redis.default-ttl=PT10M",
        ] {
            assert!(
                properties.contains(property),
                "{} lost {property}",
                subject.name
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for artifact in [
            "spring-boot-starter-data-redis",
            "testcontainers",
            "spring-boot-testcontainers",
            "maven-failsafe-plugin",
        ] {
            assert!(build.contains(artifact), "{} lost {artifact}", subject.name);
        }
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn kafka_pack_keeps_all_java_and_compose_edits_in_the_generate_edit_generate_loop() {
    let subjects = spring_subjects("kafka-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "kafka", "--no-start"]);
        let files = [
            source_unit_path(subject, "main", "messaging", "KafkaConfig.java"),
            source_unit_path(subject, "main", "messaging", "NonRetryableException.java"),
            source_unit_path(subject, "test", "messaging", "KafkaConfigTest.java"),
            source_unit_path(subject, "test", "", "KafkaTestcontainersConfig.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("kafka-reader-edit-{index}"));
        }

        let compose = subject.root.join("compose.yaml");
        let original_compose = fs::read_to_string(&compose).unwrap();
        assert!(original_compose.contains("image: apache/kafka:4.1.0"));
        let edited_compose = original_compose
            .replace(
                "    healthcheck:\n",
                "    restart: unless-stopped\n    healthcheck:\n",
            )
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            );
        fs::write(&compose, edited_compose).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("kafka-reader-edit-{index}")),
                "{} lost the Kafka edit in {}",
                subject.name,
                path.display()
            );
        }
        let compose_after = fs::read_to_string(&compose).unwrap();
        for reader_edit in [
            "restart: unless-stopped",
            "reader-service:",
            "image: reader:latest",
        ] {
            assert!(
                compose_after.contains(reader_edit),
                "{} lost `{reader_edit}` from compose.yaml: {compose_after}",
                subject.name
            );
        }

        let properties = fs::read_to_string(
            subject
                .root
                .join("src/main/resources/application.properties"),
        )
        .unwrap();
        for property in [
            "spring.kafka.consumer.group-id=demo",
            "spring.kafka.consumer.auto-offset-reset=earliest",
            "spring.kafka.consumer.properties.spring.json.trusted.packages=com.example.demo,com.example.demo.*",
            "spring.kafka.consumer.properties.group.protocol=consumer",
        ] {
            assert!(
                properties.contains(property),
                "{} lost {property}",
                subject.name
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for artifact in [
            "spring-boot-starter-kafka",
            "micrometer-core",
            "spring-boot-testcontainers",
            "testcontainers-kafka",
            "testcontainers-junit-jupiter",
            "awaitility",
        ] {
            assert!(build.contains(artifact), "{} lost {artifact}", subject.name);
        }
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn mail_pack_keeps_java_and_mailpit_edits_in_the_generate_edit_generate_loop() {
    let subjects = spring_subjects("mail-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "mail", "--no-start"]);
        let files = [
            source_unit_path(subject, "main", "", "Mailer.java"),
            source_unit_path(subject, "test", "", "MailerIT.java"),
        ];
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("mail-reader-edit-{index}"));
        }

        let compose = subject.root.join("compose.yaml");
        let original_compose = fs::read_to_string(&compose).unwrap();
        assert!(original_compose.contains("image: axllent/mailpit:v1.21"));
        let edited_compose = original_compose
            .replace("    ports:\n", "    restart: unless-stopped\n    ports:\n")
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            );
        fs::write(&compose, edited_compose).unwrap();

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("mail-reader-edit-{index}")),
                "{} lost the Mail edit in {}",
                subject.name,
                path.display()
            );
        }
        let compose_after = fs::read_to_string(&compose).unwrap();
        for reader_edit in [
            "restart: unless-stopped",
            "reader-service:",
            "image: reader:latest",
        ] {
            assert!(
                compose_after.contains(reader_edit),
                "{} lost `{reader_edit}` from compose.yaml: {compose_after}",
                subject.name
            );
        }

        let properties = fs::read_to_string(
            subject
                .root
                .join("src/main/resources/application.properties"),
        )
        .unwrap();
        for property in [
            "spring.mail.host=localhost",
            "spring.mail.port=1025",
            "app.mail.from=no-reply@example.com",
        ] {
            assert!(
                properties.contains(property),
                "{} lost {property}",
                subject.name
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for artifact in [
            "spring-boot-starter-mail",
            "spring-boot-starter-mail-test",
            "awaitility",
            "testcontainers",
            "testcontainers-junit-jupiter",
            "maven-failsafe-plugin",
        ] {
            assert!(build.contains(artifact), "{} lost {artifact}", subject.name);
        }
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn toxiproxy_pack_keeps_both_testkit_edits_and_removes_only_its_boundary() {
    let subjects = subjects("toxiproxy-capability-pack");
    for subject in &subjects {
        subject.succeeds(&["add", "toxiproxy"]);
        let files = [
            source_unit_path(subject, "test", "testkit", "Faults.java"),
            source_unit_path(subject, "test", "testkit", "FaultsTest.java"),
        ];
        let originals = files
            .iter()
            .map(|path| fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        for (index, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("toxiproxy-reader-edit-{index}"));
        }

        subject.succeeds(&["add", "fake"]);
        for (index, path) in files.iter().enumerate() {
            assert!(
                fs::read_to_string(path)
                    .unwrap()
                    .contains(&format!("toxiproxy-reader-edit-{index}")),
                "{} lost the Toxiproxy edit in {}",
                subject.name,
                path.display()
            );
        }
        let build = fs::read_to_string(subject.root.join("pom.xml")).unwrap();
        for artifact in ["testcontainers-toxiproxy", "toxiproxy-java"] {
            assert!(build.contains(artifact), "{} lost {artifact}", subject.name);
        }

        for (path, original) in files.iter().zip(originals) {
            fs::write(path, original).unwrap();
        }
        subject.succeeds(&["remove", "toxiproxy", "--force"]);
        assert!(files.iter().all(|path| !path.exists()));
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake pack",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn coverage_build_feature_preserves_reader_build_edits_and_removes_cleanly() {
    let subjects = subjects("coverage-build-feature");
    for subject in &subjects {
        subject.succeeds(&["add", "coverage"]);
        let pom = subject.root.join("pom.xml");
        let installed = fs::read_to_string(&pom).unwrap();
        for expected in [
            "jacoco-maven-plugin",
            "<version>0.8.15</version>",
            "<minimum>0.80</minimum>",
        ] {
            assert!(
                installed.contains(expected),
                "{}: {installed}",
                subject.name
            );
        }
        let reader_edited = installed.replace(
            "</project>",
            "    <!-- reader-owned-coverage-note -->\n</project>",
        );
        fs::write(&pom, &reader_edited).unwrap();

        subject.succeeds(&["add", "fake"]);
        let after_generation = fs::read_to_string(&pom).unwrap();
        assert!(
            after_generation.contains("reader-owned-coverage-note"),
            "{} lost the reader POM edit",
            subject.name
        );

        subject.succeeds(&["remove", "coverage", "--force"]);
        let removed = fs::read_to_string(&pom).unwrap();
        assert!(!removed.contains("jacoco-maven-plugin"), "{removed}");
        assert!(removed.contains("reader-owned-coverage-note"), "{removed}");
        assert!(
            source_unit_path(subject, "test", "testkit", "Fake.java").is_file(),
            "{} removed the independent fake boundary",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn loadtest_project_files_keep_reader_edits_when_routes_are_regenerated() {
    let subjects = spring_subjects("cap-loadtest-project-files");
    for subject in &subjects {
        subject.succeeds(&["g", "controller", "Health", "--path", "/health"]);
        subject.succeeds(&["add", "loadtest", "--no-start"]);
        let readme = subject.root.join("load-tests/README.md");
        let token = subject.root.join("load-tests/token-cache.js");
        for (path, edit) in [
            (&readme, "\nReader load-test notes.\n"),
            (&token, "\nexport const readerTokenHook = true;\n"),
        ] {
            let mut source = fs::read_to_string(path).unwrap();
            source.push_str(edit);
            fs::write(path, source).unwrap();
        }

        subject.succeeds(&["g", "controller", "Health", "--path", "/healthz"]);
        subject.succeeds(&["add", "loadtest", "--no-start"]);
        let api = fs::read_to_string(subject.root.join("load-tests/api.js")).unwrap();
        assert!(
            api.contains("path: \"/healthz\""),
            "{}: {api}",
            subject.name
        );
        assert!(
            fs::read_to_string(&readme)
                .unwrap()
                .contains("Reader load-test notes."),
            "{} lost README edits",
            subject.name
        );
        assert!(
            fs::read_to_string(&token)
                .unwrap()
                .contains("readerTokenHook"),
            "{} lost token-cache edits",
            subject.name
        );

        fs::write(
            &readme,
            include_str!("golden/cap-loadtest/load-tests/README.md"),
        )
        .unwrap();
        fs::write(
            &token,
            include_str!("golden/cap-loadtest/load-tests/token-cache.js"),
        )
        .unwrap();
        subject.succeeds(&["remove", "loadtest", "--force"]);
        assert!(
            !subject.root.join("load-tests").exists()
                || snapshot(&subject.root.join("load-tests")).is_empty(),
            "{} left load-test files after clean removal",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn factory_is_an_entity_projection_instead_of_a_legacy_recipe_dead_end() {
    let subjects = subjects("entity-factory");
    for subject in &subjects {
        subject.succeeds(&["g", "record", "Task", "title:string!"]);
        subject.succeeds(&["g", "factory", "Task"]);
        let factory = if subject.name == "canonical" {
            subject
                .root
                .join(".jails/generated/test/java/com/example/demo/testkit/TaskFactory.java")
        } else {
            subject
                .root
                .join("src/test/java/com/example/demo/testkit/TaskFactory.java")
        };
        let source = fs::read_to_string(&factory).unwrap();
        let anchor = "public final class TaskFactory {\n";
        assert!(source.contains(anchor), "{}: {source}", subject.name);
        fs::write(
            &factory,
            source.replace(
                anchor,
                &format!("{anchor}\n    public String readerMethod() {{ return \"reader\"; }}\n"),
            ),
        )
        .unwrap();

        let before_field = reader_snapshot(&subject.root);
        let field = subject.run(&["g", "field", "Task", "done:boolean"]);
        if subject.name == "legacy" {
            assert!(
                !field.status.success(),
                "legacy recipe replay unexpectedly worked"
            );
            assert!(
                String::from_utf8_lossy(&field.stderr)
                    .contains("factory Task reads the existing record"),
                "{}",
                String::from_utf8_lossy(&field.stderr)
            );
            assert_eq!(
                reader_snapshot(&subject.root),
                before_field,
                "legacy factory refusal wrote reader-visible state"
            );
        } else {
            assert!(
                field.status.success(),
                "canonical field evolution failed:\n{}{}",
                String::from_utf8_lossy(&field.stdout),
                String::from_utf8_lossy(&field.stderr)
            );
        }
        let evolved = fs::read_to_string(&factory).unwrap();
        assert!(
            evolved.contains("readerMethod()"),
            "{} lost its factory method: {evolved}",
            subject.name
        );
        if subject.name == "canonical" {
            assert!(
                evolved.contains("withDone(boolean value)"),
                "canonical did not evolve its factory: {evolved}"
            );
        } else {
            assert!(!evolved.contains("withDone(boolean value)"), "{evolved}");
        }
        let first_factory = fs::read(&factory).unwrap();
        let first_record = fs::read(&subject.record).unwrap();
        subject.succeeds(&["g", "factory", "Task"]);
        assert_eq!(
            fs::read(&factory).unwrap(),
            first_factory,
            "{} changed the factory bytes on a factory rerun",
            subject.name
        );
        assert_eq!(
            fs::read(&subject.record).unwrap(),
            first_record,
            "{} changed the record bytes on a factory rerun",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn repository_facet_preserves_the_legacy_iterative_contract() {
    let subjects = subjects("entity-repository");
    for subject in &subjects {
        subject.succeeds(&["g", "record", "Task", "id:string!@pk", "title:string!"]);
        subject.succeeds(&["g", "repo", "Task"]);
        let repository = if subject.name == "canonical" {
            subject
                .root
                .join(".jails/generated/main/java/com/example/demo/repository/TaskRepository.java")
        } else {
            subject
                .root
                .join("src/main/java/com/example/demo/app/TaskRepository.java")
        };
        add_reader_comment(&repository, "repository-reader-method");

        let field = subject.run(&["g", "field", "Task", "done:boolean"]);
        assert!(
            field.status.success(),
            "{} field evolution failed:\n{}{}",
            subject.name,
            String::from_utf8_lossy(&field.stdout),
            String::from_utf8_lossy(&field.stderr)
        );
        assert!(
            fs::read_to_string(&subject.record)
                .unwrap()
                .contains("boolean done"),
            "{} record did not evolve",
            subject.name
        );
        assert!(
            fs::read_to_string(&repository)
                .unwrap()
                .contains("repository-reader-method"),
            "{} lost the repository edit",
            subject.name
        );

        let first_repository = fs::read(&repository).unwrap();
        subject.succeeds(&["g", "repo", "Task"]);
        assert_eq!(
            fs::read(&repository).unwrap(),
            first_repository,
            "{} changed repository bytes on an identical rerun",
            subject.name
        );

        let clean = fs::read_to_string(&repository)
            .unwrap()
            .replace("\n\n    // repository-reader-method", "");
        fs::write(&repository, clean).unwrap();
        subject.succeeds(&["destroy", "repo", "Task", "--force"]);
        assert!(
            !repository.exists(),
            "{} did not remove its repository port",
            subject.name
        );
        assert!(
            subject.record.exists(),
            "{} repository destroy removed its record ABI",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

fn source_unit_path(subject: &Subject, source_set: &str, package: &str, file: &str) -> PathBuf {
    let root = if subject.name == "canonical" {
        format!(".jails/generated/{source_set}/java/com/example/demo")
    } else {
        format!("src/{source_set}/java/com/example/demo")
    };
    let mut path = subject.root.join(root);
    if !package.is_empty() {
        path.push(package);
    }
    path.join(file)
}

fn add_reader_comment(path: &Path, comment: &str) {
    let source = fs::read_to_string(path).unwrap();
    let edited = if let Some(split) = source.rfind("\n}") {
        format!(
            "{}\n\n    // {comment}{}",
            &source[..split],
            &source[split..]
        )
    } else {
        let split = source.rfind("{}").expect("Java type has no closing body");
        format!(
            "{}{{\n\n    // {comment}\n}}{}",
            &source[..split],
            &source[split + 2..]
        )
    };
    fs::write(path, edited).unwrap();
}

#[test]
fn identical_generation_reruns_and_destroy_are_semantically_symmetric() {
    let subjects = subjects("rerun-destroy-record");
    for subject in &subjects {
        let generate = ["g", "record", "Task", "title:string!"];
        subject.succeeds(&generate);
        let first = reader_snapshot(&subject.root);
        subject.succeeds(&generate);
        assert_eq!(
            reader_snapshot(&subject.root),
            first,
            "{} changed its reader-visible project tree on an identical rerun",
            subject.name
        );

        subject.succeeds(&["destroy", "record", "Task", "--force"]);
        assert!(
            !subject.record.exists(),
            "{} left the generated record after destroy",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn operation_reruns_and_destroy_are_semantically_symmetric() {
    let subjects = spring_subjects("rerun-destroy-operation");
    for subject in &subjects {
        subject.succeeds(&["g", "record", "Task", "id:uuid@pk", "title:string!"]);
        let generate = [
            "g",
            "usecase",
            "CreateTask",
            "title:string!",
            "--on",
            "Task",
        ];
        subject.succeeds(&generate);
        assert!(
            tree_mentions(&subject.root, "CreateTask"),
            "{} generated no CreateTask artifact",
            subject.name
        );
        let first = reader_snapshot(&subject.root);
        subject.succeeds(&generate);
        assert_eq!(
            reader_snapshot(&subject.root),
            first,
            "{} changed reader-visible state on an identical operation rerun",
            subject.name
        );

        subject.succeeds(&["destroy", "usecase", "CreateTask", "--force"]);
        assert!(
            !tree_mentions(&subject.root, "CreateTask"),
            "{} left a CreateTask artifact after destroy",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn value_is_a_record_profile_with_symmetric_rerun_and_destroy() {
    let subjects = subjects("value-profile");
    for subject in &subjects {
        let generate = ["g", "value", "Money", "amount:long", "currency:string!"];
        subject.succeeds(&generate);
        assert!(
            tree_mentions(&subject.root, "Money"),
            "{} generated no Money artifact",
            subject.name
        );
        let first = reader_snapshot(&subject.root);
        subject.succeeds(&generate);
        assert_eq!(
            reader_snapshot(&subject.root),
            first,
            "{} changed reader-visible state on an identical value rerun",
            subject.name
        );
        subject.succeeds(&["destroy", "value", "Money", "--force"]);
        assert!(
            !tree_mentions(&subject.root, "Money"),
            "{} left a Money artifact after destroy",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn dto_rerun_preserves_edits_in_request_response_and_contract_test() {
    let subjects = spring_subjects("dto-three-file-loop");
    for subject in &subjects {
        subject.succeeds(&[
            "g",
            "record",
            "Task",
            "id:int@pk",
            "title:string!",
            "note:string?",
        ]);
        subject.succeeds(&["g", "dto", "Task"]);
        let files = [
            source_unit_path(subject, "main", "web", "TaskRequest.java"),
            source_unit_path(subject, "main", "web", "TaskResponse.java"),
            source_unit_path(subject, "test", "web", "TaskDtoTest.java"),
        ];
        for (number, path) in files.iter().enumerate() {
            add_reader_comment(path, &format!("dto-reader-edit-{number}"));
        }

        let stable = files
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>();
        subject.succeeds(&["g", "dto", "Task"]);
        for (number, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            assert!(
                source.contains(&format!("dto-reader-edit-{number}")),
                "{} lost the DTO edit in {}",
                subject.name,
                path.display()
            );
            assert!(source.contains("Task"), "{}: {source}", subject.name);
        }
        assert_eq!(
            files
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            stable,
            "{} changed DTO bytes on an identical rerun",
            subject.name
        );

        for (number, path) in files.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            fs::write(
                path,
                source.replace(&format!("\n\n    // dto-reader-edit-{number}"), ""),
            )
            .unwrap();
        }
        subject.succeeds(&["destroy", "dto", "Task", "--force"]);
        assert!(
            files.iter().all(|path| !path.exists()),
            "{} left DTO artifacts after destroy",
            subject.name
        );
        assert!(
            subject.record.exists(),
            "{} DTO destroy removed its domain record",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn scaffold_evolution_preserves_edits_in_every_generated_java_artifact() {
    let subjects = spring_subjects("scaffold-all-artifacts");
    for subject in &subjects {
        subject.succeeds(&["g", "scaffold", "Task", "id:uuid@pk", "title:string!"]);
        let artifacts = java_artifacts_named(&subject.root, "Task");
        assert!(
            artifacts.len() >= 4,
            "{} scaffold emitted only {} Task Java artifacts: {artifacts:?}",
            subject.name,
            artifacts.len()
        );
        for (number, path) in artifacts.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            let split = source
                .rfind("\n}")
                .unwrap_or_else(|| panic!("{} has no top-level closing brace", path.display()));
            fs::write(
                path,
                format!(
                    "{}\n\n    // reader-edit-{number}{}",
                    &source[..split],
                    &source[split..]
                ),
            )
            .unwrap();
        }

        subject.succeeds(&["g", "field", "Task", "done:boolean?"]);
        for (number, path) in artifacts.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap_or_else(|error| {
                panic!(
                    "{} removed edited artifact {}: {error}",
                    subject.name,
                    path.display()
                )
            });
            assert!(
                source.contains(&format!("reader-edit-{number}")),
                "{} lost the hand edit in {}",
                subject.name,
                path.display()
            );
        }
        assert!(
            fs::read_to_string(&subject.record)
                .unwrap()
                .contains("Optional<Boolean> done"),
            "{} did not evolve the scaffold record",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[test]
fn enum_artifacts_keep_reader_edits_until_explicit_destroy() {
    let subjects = subjects("enum-all-artifacts");
    for subject in &subjects {
        let generate = ["g", "enum", "Status", "OPEN", "IN_PROGRESS=in_progress"];
        subject.succeeds(&generate);
        let artifacts = java_artifacts_named(&subject.root, "Status");
        assert!(
            !artifacts.is_empty(),
            "{} generated no Status Java artifacts",
            subject.name
        );
        for (number, path) in artifacts.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            let split = source
                .rfind("\n}")
                .unwrap_or_else(|| panic!("{} has no top-level closing brace", path.display()));
            fs::write(
                path,
                format!(
                    "{}\n\n    // enum-reader-edit-{number}{}",
                    &source[..split],
                    &source[split..]
                ),
            )
            .unwrap();
        }

        subject.succeeds(&["g", "record", "Task", "title:string!"]);
        subject.succeeds(&generate);
        for (number, path) in artifacts.iter().enumerate() {
            let source = fs::read_to_string(path).unwrap();
            assert!(
                source.contains(&format!("enum-reader-edit-{number}")),
                "{} lost the enum edit in {}",
                subject.name,
                path.display()
            );
            fs::write(
                path,
                source.replace(&format!("\n\n    // enum-reader-edit-{number}"), ""),
            )
            .unwrap();
        }

        subject.succeeds(&["destroy", "enum", "Status", "--force"]);
        assert!(
            java_artifacts_named(&subject.root, "Status").is_empty(),
            "{} left Status Java artifacts after destroy",
            subject.name
        );
    }

    for subject in subjects {
        fs::remove_dir_all(subject.root).ok();
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FileImage {
    bytes: Vec<u8>,
    executable: bool,
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, FileImage> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn reader_snapshot(root: &Path) -> BTreeMap<PathBuf, FileImage> {
    let mut files = snapshot(root);
    files.retain(|path, _| !private_executor_state(path));
    files
}

fn private_executor_state(path: &Path) -> bool {
    let path = path.to_string_lossy().replace('\\', "/");
    [
        ".jails/objects/",
        ".jails/transactions/",
        ".jails/receipts/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || matches!(path.as_str(), ".jails/lock" | ".jails/effects.lock")
}

fn tree_mentions(root: &Path, needle: &str) -> bool {
    snapshot(root)
        .keys()
        .any(|path| path.to_string_lossy().contains(needle))
}

fn java_artifacts_named(root: &Path, name: &str) -> Vec<PathBuf> {
    snapshot(root)
        .into_keys()
        .filter(|path| {
            path.extension() == Some(OsStr::new("java"))
                && path
                    .file_name()
                    .is_some_and(|file_name| file_name.to_string_lossy().contains(name))
        })
        .map(|path| root.join(path))
        .collect()
}

fn collect(root: &Path, at: &Path, files: &mut BTreeMap<PathBuf, FileImage>) {
    let entries = fs::read_dir(at)
        .unwrap_or_else(|error| panic!("could not snapshot {}: {error}", at.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name() != Some(OsStr::new("target")) {
                collect(root, &path, files);
            }
            continue;
        }
        let relative = path.strip_prefix(root).unwrap().to_path_buf();
        files.insert(
            relative,
            FileImage {
                bytes: fs::read(&path).unwrap(),
                executable: executable(&path),
            },
        );
    }
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> bool {
    false
}

/// Two subjects over a project jails did not write.
///
/// The canonical side gets a `.jails/model.jdl` naming the *reader's* base
/// package, not `com.example.demo`: the whole point of an adopted project is
/// that jails did not choose where anything lives.
fn adopted_subjects(label: &str, flavour: Adopted) -> [Subject; 2] {
    let legacy_root = temp_dir(&format!("differential-{label}-legacy"));
    let canonical_root = temp_dir(&format!("differential-{label}-canonical"));
    write_adopted_fixture(&legacy_root, flavour);
    write_adopted_fixture(&canonical_root, flavour);
    // No `model.jdl` yet, deliberately. `adopt` refuses on a canonical project
    // -- "adopt only before creating the model" -- and it is right to: adoption
    // is how jails learns a layout it did not choose, which has to happen
    // before there is a model that claims to know one. The test opts the
    // canonical subject in after adopting, which is the real order a reader
    // would follow.

    [
        Subject {
            name: "legacy",
            binary: std::env::var_os("JAILS_LEGACY_BIN")
                .unwrap_or_else(|| OsString::from(env!("CARGO_BIN_EXE_jails"))),
            record: legacy_root.join("src/main/java/net/acme/legacy/domain/Receipt.java"),
            root: legacy_root,
        },
        Subject {
            name: "canonical",
            binary: OsString::from(env!("CARGO_BIN_EXE_jails")),
            record: canonical_root
                .join(".jails/generated/main/java/net/acme/legacy/domain/Receipt.java"),
            root: canonical_root,
        },
    ]
}

/// Both implementations treat a foreign codebase the same way.
///
/// `simplify-sol.md`'s G5, differential half. `tests/cli/tooling.rs` proves the
/// current binary handles an adopted project; this proves the *replacement*
/// handles it the way the thing it replaces did -- which is the question a
/// cutover actually turns on, and the one a single-binary test cannot ask.
///
/// Under `scripts/verify-rewrite-g1-canary.sh` the legacy side is a binary
/// built from a frozen revision, so this keeps meaning something after the
/// legacy crates are deleted.
///
/// What is compared is behaviour, not private state: the two keep their
/// generated Java in different places by design, so the assertions are about
/// what the *reader* can see -- their own bytes, and whether a rerun settles.
#[test]
fn an_adopted_project_is_treated_the_same_by_both_implementations() {
    for flavour in [Adopted::Plain, Adopted::Spring] {
        adopted_project_is_treated_the_same(flavour);
    }
}

/// One flavour of [`adopted_subjects`], run through both implementations.
///
/// The Spring flavour is not a copy with a bigger pom. Every version fact
/// jails renders against -- repository wiring, the MockMvc form, the
/// webmvc-test module, whether `package-info.java` may be annotated -- is read
/// off the *reader's* build file, so a Spring project jails did not create is
/// where a wrong reading turns into Java that does not compile.
fn adopted_project_is_treated_the_same(flavour: Adopted) {
    let label = match flavour {
        Adopted::Plain => "adopted-plain",
        Adopted::Spring => "adopted-spring",
    };
    let subjects = adopted_subjects(label, flavour);
    for subject in &subjects {
        let before = adopted_reader_bytes(&subject.root, flavour);

        subject.succeeds(&["adopt"]);
        assert_eq!(
            adopted_reader_bytes(&subject.root, flavour),
            before,
            "{label}: {} rewrote the reader's source while adopting",
            subject.name
        );

        // Both implementations learn the reader's layout identically: the
        // fixture keeps its adapters in `persistence`, and adoption records
        // that rename rather than reporting the directory as unrecognised.
        // What each then *does* with it is
        // `both_implementations_write_adapters_into_the_reader_s_own_package`.
        assert!(
            fs::read_to_string(subject.root.join("jails.toml"))
                .unwrap()
                .contains(r#"adapters = "persistence""#),
            "{label}: {} did not record the reader's adapters directory",
            subject.name
        );

        if subject.name == "canonical" {
            fs::write(
                subject.root.join(".jails/model.jdl"),
                "application Orders @id(project_orders)\n\
                 package net.acme.legacy\njava 26\ndialect postgresql\n",
            )
            .unwrap();
        }

        subject.succeeds(&["g", "record", "Receipt", "id:uuid", "total:long"]);
        assert!(
            subject.record.is_file(),
            "{} did not write {}",
            subject.name,
            subject.record.display()
        );
        let generated = fs::read_to_string(&subject.record).unwrap();
        assert!(
            generated.contains("package net.acme.legacy.domain;"),
            "{} put the record outside the reader's package: {generated}",
            subject.name
        );
        assert_eq!(
            adopted_reader_bytes(&subject.root, flavour),
            before,
            "{label}: {} rewrote a file it did not author",
            subject.name
        );

        // A rerun settles rather than rewriting. Identity is the entity, so
        // re-declaring the same record is an update that changes nothing.
        let again = subject.run(&["g", "record", "Receipt", "id:uuid", "total:long"]);
        assert!(
            again.status.success(),
            "{} failed on a rerun: {}",
            subject.name,
            String::from_utf8_lossy(&again.stderr)
        );
        assert_eq!(
            fs::read_to_string(&subject.record).unwrap(),
            generated,
            "{} rewrote its own output on a rerun",
            subject.name
        );
        assert_eq!(
            adopted_reader_bytes(&subject.root, flavour),
            before,
            "{label}: {} rewrote the reader's source on a rerun",
            subject.name
        );

        // The reader's own directory names survive: `persistence`, which jails
        // would have called `adapters`.
        assert!(
            adopted_base(&subject.root).join("persistence").is_dir(),
            "{} moved the reader's persistence package",
            subject.name
        );
    }
}

/// A rename `adopt` recorded reaches the generated code, on both sides.
///
/// This was `bugs.md` B59, found by the differential fixture above and fixed
/// in the same change: the canonical compiler named its packages with 28
/// hardcoded `format!("{}.adapters.jdbc", base_package)` sites, none of which
/// could apply a rename because none of them knew there was one. So `jails
/// adopt` printed its mapping, wrote `jails.toml`, and changed nothing about
/// where a canonical project's code went.
///
/// The two reach the same place by different routes, which is what makes this
/// a differential test rather than two assertions: the legacy scaffold emits
/// its own in-memory adapter, and the canonical one is the `fake` capability.
/// What has to agree is the package the reader ends up importing.
#[test]
fn both_implementations_write_adapters_into_the_reader_s_own_package() {
    let subjects = adopted_subjects("adopted-layout", Adopted::Spring);
    for subject in &subjects {
        subject.succeeds(&["adopt"]);
        if subject.name == "canonical" {
            fs::write(
                subject.root.join(".jails/model.jdl"),
                "application Orders @id(project_orders)\n\
                 package net.acme.legacy\njava 26\ndialect postgresql\n",
            )
            .unwrap();
        }
        subject.succeeds(&[
            "g",
            "scaffold",
            "Invoice",
            "id:uuid@pk",
            "number:string!",
            "total:long",
        ]);
        if subject.name == "canonical" {
            subject.succeeds(&["add", "fake"]);
        }

        let adapters: Vec<String> = walk_java(&subject.root)
            .into_iter()
            .filter(|path| path.contains("InMemoryInvoiceRepository"))
            .collect();
        assert_eq!(
            adapters.len(),
            1,
            "{}: expected exactly one in-memory adapter, got {adapters:?}",
            subject.name
        );
        assert!(
            adapters[0].contains("/persistence/"),
            "{}: adapter ignored the reader's `persistence` directory: {}",
            subject.name,
            adapters[0]
        );
        assert!(
            !adapters[0].contains("/adapters/"),
            "{}: adapter used jails' own layer name: {}",
            subject.name,
            adapters[0]
        );
    }
}

/// Every `.java` file under a project root, as slash-separated relative paths.
fn walk_java(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java") {
                found.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    found.sort();
    found
}

/// The CI workflow is one file, produced by one template, on both sides.
///
/// `plan.md` P13.8: `ci` was one of the four capabilities with no canonical
/// backend. Proving it byte-for-byte matters more here than for a Java
/// artifact, because the workflow pins its actions *by commit* -- a tag is
/// mutable and a moved tag is a supply chain compromise nobody sees in a diff
/// -- so two engines rendering "a CI file" from two copies of the bytes is
/// exactly how one of them ends up running an action nobody re-reviewed.
///
/// The wrapper is why this capability needed an observed workspace fact at
/// all, and both halves are asserted: `./mvnw` on a project without one fails
/// at the first step, and `mvn` on a project with one silently uses whatever
/// Maven the runner happens to have.
#[test]
fn the_ci_workflow_is_byte_identical_on_both_implementations() {
    for wrapper in [false, true] {
        let subjects = spring_subjects(if wrapper { "ci-mvnw" } else { "ci-plain" });
        let mut rendered = Vec::new();
        for subject in &subjects {
            if wrapper {
                fs::write(subject.root.join("mvnw"), "#!/bin/sh\n").unwrap();
            }
            subject.succeeds(&["add", "ci"]);
            let workflow = subject.root.join(".github/workflows/ci.yml");
            let text = fs::read_to_string(&workflow).unwrap_or_else(|error| {
                panic!("{}: {} — {error}", subject.name, workflow.display())
            });
            assert!(
                text.contains("uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd"),
                "{}: the checkout action is not pinned by commit: {text}",
                subject.name
            );
            let expected = if wrapper { "./mvnw" } else { "mvn" };
            assert!(
                text.contains(&format!("run: {expected} -B -ntp clean verify")),
                "{}: wrapper={wrapper} but the workflow runs something else: {text}",
                subject.name
            );
            rendered.push(text);
        }
        assert_eq!(
            rendered[0], rendered[1],
            "the two implementations render different CI workflows (wrapper={wrapper})"
        );
        for subject in subjects {
            fs::remove_dir_all(subject.root).ok();
        }
    }
}

/// The container build is the same three files on both implementations.
///
/// `plan.md` P13.8's second capability. The assertions are the two properties
/// the image is *for*: it runs as a numeric non-root user, and the workflow
/// checks that rather than trusting it. A container that quietly starts
/// running as root is the failure this capability exists to prevent, and it is
/// invisible until something else goes wrong.
#[test]
fn the_container_build_is_byte_identical_on_both_implementations() {
    for wrapper in [false, true] {
        let subjects = spring_subjects(if wrapper {
            "docker-mvnw"
        } else {
            "docker-plain"
        });
        let mut rendered: Vec<Vec<String>> = Vec::new();
        for subject in &subjects {
            if wrapper {
                fs::write(subject.root.join("mvnw"), "#!/bin/sh\n").unwrap();
            }
            subject.succeeds(&["add", "docker"]);
            let files = ["Dockerfile", ".dockerignore", ".github/workflows/image.yml"]
                .map(|path| {
                    fs::read_to_string(subject.root.join(path))
                        .unwrap_or_else(|error| panic!("{}: {path} — {error}", subject.name))
                })
                .to_vec();
            assert!(
                files[0].contains("USER 10001:10001"),
                "{}: the image does not drop to a numeric non-root user: {}",
                subject.name,
                files[0]
            );
            assert!(
                files[2].contains("{{.Config.User}}"),
                "{}: the workflow does not check the runtime user, so nothing would notice: {}",
                subject.name,
                files[2]
            );
            let builder = if wrapper { "./mvnw -B" } else { "mvn -B" };
            assert!(
                files[0].contains(builder),
                "{}: wrapper={wrapper} but the build stage runs something else: {}",
                subject.name,
                files[0]
            );
            rendered.push(files);
        }
        assert_eq!(
            rendered[0], rendered[1],
            "the two implementations render different container builds (wrapper={wrapper})"
        );
        for subject in subjects {
            fs::remove_dir_all(subject.root).ok();
        }
    }
}
