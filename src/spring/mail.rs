//! `add mail`: sending, and a test that proves the message arrived.
//!
//! Two things this exists to fix, and neither is about the API.
//!
//! **A mail test that asserts `send()` did not throw proves almost nothing.** A
//! wrong From, a wrong recipient, an empty subject and a message the server
//! silently drops all pass it. Spring Boot's own
//! `MailSenderAutoConfigurationIntegrationTests` does not make that mistake —
//! it starts Mailpit, sends over SMTP and reads the inbox back over POP3 — and
//! the generated IT is that shape, because it is the shape that can fail.
//!
//! **`spring.mail.host` has no default that fails loudly.** Unset, JavaMail
//! falls back to `localhost:25`, so a deployment that forgot to configure it
//! does not fail at startup: it fails at the first send, per message, in
//! whatever thread was sending. The properties set it explicitly and say so.
//!
//! One difference from `add db` worth stating, because it is why the test looks
//! different: **there is no `@ServiceConnection` for mail** — no
//! `MailConnectionDetails` exists anywhere in `deps/spring-boot` — so the host
//! and port are bound with `@DynamicPropertySource` instead.
//!
//! `spring-boot-starter-mail-test` is Boot 4's `-test` twin convention: the
//! starter plus `spring-boot-starter-test`, so the test scope gets both from
//! one line.
//!
//! The send-and-read-back path was run against a live Mailpit, not only
//! compiled: message sent over SMTP, read back over POP3, subject matched.
//! Compilation alone would have proved nothing here, since every failure this
//! test exists to catch compiles.

use super::*;

pub(crate) const MAIL_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-mail",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const MAIL_TEST_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-mail-test",
    version: None,
    scope: Some("test"),
    optional: false,
};

/// Awaitility: versionless because `spring-boot-dependencies` manages
/// `org.awaitility`, and pinning it beside a BOM that moves is how two
/// versions of one library end up on a classpath.
pub(crate) const AWAITILITY: Dependency = Dependency {
    group_id: "org.awaitility",
    artifact_id: "awaitility",
    version: None,
    scope: Some("test"),
    optional: false,
};

/// The image, in one place: the compose service and the integration test start
/// the same server, so what you read in the browser and what the test reads
/// over POP3 cannot be two different things.
const IMAGE: &str = "axllent/mailpit:v1.21";

pub(crate) fn mail_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let name = "Mailer";
    Change {
        deps: vec![
            MAIL_STARTER,
            MAIL_TEST_STARTER,
            AWAITILITY,
            // Shared with `add db` and `add redis` rather than redeclared:
            // one artifact named twice is one artifact that gets bumped once.
            TESTCONTAINERS_CORE,
            TESTCONTAINERS_JUNIT,
        ],
        files: vec![
            artifact(
                crate::generate::main_dir(root, pkg).join(format!("{name}.java")),
                crate::template::render(
                    crate::template_here!("spring/mailer_java.java"),
                    &[("pkg", pkg), ("name", name)],
                ),
            ),
            artifact(
                crate::generate::test_dir(root, pkg).join(format!("{name}IT.java")),
                crate::template::render(
                    crate::template_here!("spring/mailer_it_java.java"),
                    &[("pkg", pkg), ("name", name), ("image", IMAGE)],
                ),
            ),
        ],
        compose: vec![crate::compose::MAILPIT],
        properties: vec![
            "# Where mail goes. Set explicitly because JavaMail's fallback is".to_string(),
            "# localhost:25, which fails at the first send rather than at startup.".to_string(),
            "spring.mail.host=localhost".to_string(),
            "spring.mail.port=1025".to_string(),
            "# One From address, set once: a per-call-site literal drifts, and the".to_string(),
            "# one that drifts is the one a receiving server rejects for failing SPF.".to_string(),
            "app.mail.from=no-reply@example.com".to_string(),
        ],
        ..Change::default()
    }
}
