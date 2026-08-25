//! `g auth`: this service issuing its own tokens, and the default it must undo.
//!
//! Two facts, both read out of `deps/` rather than remembered.
//!
//! **Spring Boot auto-configures no `JwtEncoder`** -- there is not one
//! occurrence of the type in the whole of `deps/spring-boot`. The
//! resource-server starter gives a `JwtDecoder` for *someone else's* tokens and
//! stops, so a service that issues its own has to declare the encoder.
//!
//! **A JWT with no `exp` passes the default decoder.**
//! `JwtTimestampValidator` ships `allowEmptyExpiryClaim = true`
//! (`deps/spring-security/.../JwtTimestampValidator.java:58`), so a token that
//! never expires is accepted by every out-of-the-box configuration and nothing
//! warns. The generated config turns it off, and the generated test is what
//! keeps the line there -- deleting it changes no behaviour any other test can
//! see, which is the definition of a change that survives review.

use super::*;

/// The issuer claim, and why it is derived rather than asked for.
///
/// A token's `iss` has to be stable across restarts and the same on both sides
/// of the verification, so it cannot be generated. The base package is the one
/// stable name jails already knows about the project; a real deployment
/// replaces it with a URL, and the Javadoc says so.
fn issuer_of(slice: &Slice) -> String {
    format!("urn:{}", slice.base())
}

pub(crate) fn auth_files(slice: &Slice, name: &str) -> jails_support::Result<Vec<Artifact>> {
    if !slice
        .project()
        .has_dependency("org.springframework.boot", "spring-boot-starter-security")
    {
        return Err(format!(
            "auth {name} needs Spring Security: the encoder, the decoder and the filter \
             chain that reads the token are one story.\n       \
             fix: run `jails add security` first."
        )
        .into());
    }
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let issuer = issuer_of(slice);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);

    Ok(vec![
        Artifact {
            kind: "token config",
            path: main.join(format!("{name}TokenConfig.java")),
            contents: crate::template::render(
                crate::template_here!("spring/auth_config_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
        },
        Artifact {
            kind: "token issuer",
            path: main.join(format!("{name}Tokens.java")),
            contents: crate::template::render(
                crate::template_here!("spring/auth_tokens_java.java"),
                &[("pkg", pkg), ("name", name), ("issuer", &issuer)],
            ),
        },
        Artifact {
            kind: "token issuer test",
            path: test.join(format!("{name}TokensTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/auth_tokens_test_java.java"),
                &[("pkg", pkg), ("name", name), ("issuer", &issuer)],
            ),
        },
    ])
}
