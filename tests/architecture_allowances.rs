//! Executable acceptance cases for generated ArchUnit allowance policy.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn allowances_are_bounded_current_and_used() {
    if Command::new("mvn").arg("--version").output().is_err() {
        eprintln!("skipping architecture allowance acceptance: Maven is unavailable");
        return;
    }
    let scratch = tempfile::Builder::new()
        .prefix("jails-architecture-allowances-")
        .tempdir()
        .unwrap();
    let root = scratch.path();
    copy("tests/golden/scaffold-spring/pom.xml", root.join("pom.xml"));
    copy(
        "tests/golden/scaffold-spring/src/test/java/com/example/demo/ArchitectureTest.java",
        root.join("src/test/java/com/example/demo/ArchitectureTest.java"),
    );
    copy(
        "tests/golden/scaffold-spring/src/test/resources/archunit.properties",
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
    write(
        root.join("src/main/java/com/example/demo/app/Port.java"),
        "package com.example.demo.app;\npublic interface Port {}\n",
    );
    write(
        root.join("src/main/java/com/example/demo/adapters/Adapter.java"),
        "package com.example.demo.adapters;\npublic final class Adapter {}\n",
    );
    write(
        root.join("src/main/java/com/example/demo/web/PingController.java"),
        "package com.example.demo.web;\npublic final class PingController {}\n",
    );

    policy(
        root,
        "billing",
        "com.example.demo.domain.shared.money..",
        "2099-01-31",
    );
    let accepted = architecture_test(root);
    assert!(
        accepted.status.success(),
        "valid used allowance failed:\n{}",
        diagnostics(root, &accepted)
    );

    policy(
        root,
        "orders",
        "com.example.demo.domain.shared.money..",
        "2099-01-31",
    );
    assert_failure(root, "unused architecture allowance");

    policy(root, "billing", "com.example.demo.domain..", "2099-01-31");
    assert_failure(root, "blanket or out-of-slice package pattern");

    policy(
        root,
        "billing",
        "com.example.demo.domain.shared.money..",
        "2000-01-01",
    );
    assert_failure(root, "allowance expired on 2000-01-01");
}

fn policy(root: &Path, from: &str, packages: &str, expires: &str) {
    write(
        root.join(".jails/architecture.toml"),
        &format!(
            "[[architecture.allow]]\n\
             from = \"{from}\"\n\
             to = \"shared\"\n\
             packages = [\"{packages}\"]\n\
             reason = \"reviewed acceptance edge\"\n\
             expires = \"{expires}\"\n"
        ),
    );
}

fn architecture_test(root: &Path) -> Output {
    let _ = fs::remove_dir_all(root.join("target/surefire-reports"));
    Command::new("mvn")
        .args(["-q", "-Dtest=ArchitectureTest", "test"])
        .current_dir(root)
        .output()
        .unwrap()
}

fn assert_failure(root: &Path, evidence: &str) {
    let failed = architecture_test(root);
    assert!(
        !failed.status.success(),
        "policy unexpectedly passed: {evidence}"
    );
    let diagnostics = diagnostics(root, &failed);
    assert!(
        diagnostics.contains(evidence),
        "missing `{evidence}`:\n{diagnostics}"
    );
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
