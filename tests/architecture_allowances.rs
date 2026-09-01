//! Executable acceptance cases for generated ArchUnit allowance policy.
//!
//! **Four cases, four projects, one Maven run each -- concurrently.** They
//! used to share one directory that each case rewrote `.jails/architecture.toml`
//! in, which forced them into a sequence: the second case could not start until
//! the first had finished reading the file it was about to overwrite. Four
//! `mvn test` runs at roughly 2.8s each made this a 14s test binary, all of it
//! on one thread, for four questions that have nothing to say to each other.
//!
//! Nothing about the cases required sharing. Each is a policy file and the
//! verdict ArchUnit reaches on it, and the project around it is eight small
//! files that cost microseconds to write. Giving each case its own project
//! removes the only reason they were ordered, and they go through the same
//! process-wide scheduler the rest of the suite uses -- so four Maven trees
//! here still count against the same budget as everyone else's.

#[path = "common/parallel.rs"]
mod parallel;

#[path = "common/toolchain.rs"]
mod toolchain;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// One acceptance case: the policy to write, and what the run must say.
///
/// `expect` is deliberately the *evidence string* rather than a bare
/// pass/fail: a policy that fails for the wrong reason is a passing test that
/// proves nothing, which is the failure this table is shaped to make visible.
struct Case {
    label: &'static str,
    from: &'static str,
    packages: &'static str,
    expires: &'static str,
    expect: Expect,
}

enum Expect {
    /// The build passes: the allowance is in bounds, current, and used.
    Accepted,
    /// The build fails, and its diagnostics contain this.
    Rejected(&'static str),
}

const CASES: &[Case] = &[
    Case {
        label: "valid-used-allowance",
        from: "billing",
        packages: "com.example.demo.domain.shared.money..",
        expires: "2099-01-31",
        expect: Expect::Accepted,
    },
    Case {
        label: "unused-allowance",
        from: "orders",
        packages: "com.example.demo.domain.shared.money..",
        expires: "2099-01-31",
        expect: Expect::Rejected("unused architecture allowance"),
    },
    Case {
        label: "blanket-pattern",
        from: "billing",
        packages: "com.example.demo.domain..",
        expires: "2099-01-31",
        expect: Expect::Rejected("blanket or out-of-slice package pattern"),
    },
    Case {
        label: "expired-allowance",
        from: "billing",
        packages: "com.example.demo.domain.shared.money..",
        expires: "2000-01-01",
        expect: Expect::Rejected("allowance expired on 2000-01-01"),
    },
];

#[test]
fn allowances_are_bounded_current_and_used() {
    // The gate this binary evaded twice, now taken from its one owner.
    //
    // It was first a bare `eprintln!` and a `return` -- the one thing a skip
    // must never be, since the whole point of the tier switch is that nothing
    // may report green without running. Replacing that with a hand-copied
    // assertion fixed the symptom and left the cause: two spellings of one
    // contract, in a repository whose own rules say a second copy is a copy
    // that drifts. `toolchain::skip` is that contract, and `common/mod.rs`
    // re-exports the same function to everyone else.
    if !toolchain::toolchain_enabled() || Command::new("mvn").arg("--version").output().is_err() {
        toolchain::skip("architecture allowance acceptance: Maven is unavailable");
        return;
    }

    let findings: Vec<String> = parallel::map(CASES, run_case)
        .into_iter()
        .flatten()
        .collect();

    assert!(
        findings.is_empty(),
        "{} architecture allowance case(s) are wrong:\n\n{}",
        findings.len(),
        findings.join("\n\n")
    );
}

/// One case in its own project. `None` when it behaved as declared.
fn run_case(case: &Case) -> Option<String> {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("jails-architecture-allowances-{}-", case.label))
        .tempdir()
        .unwrap();
    let root = scratch.path();
    project(root);
    write(
        root.join(".jails/architecture.toml"),
        &format!(
            "[[architecture.allow]]\n\
             from = \"{}\"\n\
             to = \"shared\"\n\
             packages = [\"{}\"]\n\
             reason = \"reviewed acceptance edge\"\n\
             expires = \"{}\"\n",
            case.from, case.packages, case.expires
        ),
    );

    let output = architecture_test(root);
    match case.expect {
        Expect::Accepted if !output.status.success() => Some(format!(
            "{}: a valid, current, used allowance was rejected:\n{}",
            case.label,
            diagnostics(root, &output)
        )),
        Expect::Accepted => None,
        Expect::Rejected(evidence) if output.status.success() => Some(format!(
            "{}: the policy was accepted, but it should have been refused for \
             `{evidence}`",
            case.label
        )),
        Expect::Rejected(evidence) => {
            let diagnostics = diagnostics(root, &output);
            if diagnostics.contains(evidence) {
                None
            } else {
                Some(format!(
                    "{}: refused, but not for `{evidence}`:\n{diagnostics}",
                    case.label
                ))
            }
        }
    }
}

/// The generated scaffold's own architecture suite, over a domain with one
/// cross-slice edge for a policy to have an opinion about.
fn project(root: &Path) {
    copy("tests/golden/scaffold-spring/pom.xml", root.join("pom.xml"));
    copy(
        "tests/golden/scaffold-spring/.jails/generated/test/java/com/example/demo/ArchitectureTest.java",
        root.join("src/test/java/com/example/demo/ArchitectureTest.java"),
    );
    copy(
        "tests/golden/scaffold-spring/.jails/generated/test/resources/archunit.properties",
        root.join("src/test/resources/archunit.properties"),
    );
    write(
        root.join("src/main/java/com/example/demo/domain/billing/Bill.java"),
        "package com.example.demo.domain.billing;\n\
         import com.example.demo.domain.shared.money.Money;\n\
         public record Bill(Money money) {}\n",
    );
    write(
        root.join("src/main/java/com/example/demo/domain/shared/money/Money.java"),
        "package com.example.demo.domain.shared.money;\n\
         import com.example.demo.domain.billing.Bill;\n\
         public record Money(Bill bill) {}\n",
    );
    // **`repository`, which is where the canonical layout puts a port.** The
    // rule under test names that package by generated text, so a fixture whose
    // port sat in `app` left `APPLICATION_PORTS_DEPEND_INWARD` matching no
    // class at all -- and ArchUnit fails a rule that checked nothing, which is
    // the whole reason it does.
    write(
        root.join("src/main/java/com/example/demo/repository/Port.java"),
        "package com.example.demo.repository;\npublic interface Port {}\n",
    );
    write(
        root.join("src/main/java/com/example/demo/adapters/Adapter.java"),
        "package com.example.demo.adapters;\npublic final class Adapter {}\n",
    );
    write(
        root.join("src/main/java/com/example/demo/web/PingController.java"),
        "package com.example.demo.web;\npublic final class PingController {}\n",
    );
}

fn architecture_test(root: &Path) -> Output {
    let _ = fs::remove_dir_all(root.join("target/surefire-reports"));
    Command::new("mvn")
        // `-DforkCount=0` for the same reason the rest of the suite uses it:
        // Surefire's default fork starts a *cold* JVM per run and re-pays class
        // loading and JIT warmup that the Maven JVM has already done. These four
        // ArchUnit policies are pure classpath analysis with no isolation to
        // lose.
        .args(["-q", "-DforkCount=0", "-Dtest=ArchitectureTest", "test"])
        .current_dir(root)
        .output()
        .unwrap()
}

fn diagnostics(root: &Path, output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if let Ok(entries) = fs::read_dir(root.join("target/surefire-reports")) {
        for entry in entries.flatten() {
            if let Ok(body) = fs::read_to_string(entry.path()) {
                text.push_str(&body);
            }
        }
    }
    text
}

fn copy(from: &str, to: PathBuf) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::copy(from, to).unwrap();
}

fn write(path: PathBuf, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}
