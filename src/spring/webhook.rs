//! `g webhook`: an inbound delivery you can believe, and the three ways that
//! belief is normally misplaced.
//!
//! The outbound half is `g http-sink`. This is the other direction, and its
//! failure modes are entirely different — every one of them is a rejection or
//! an acceptance that should have gone the other way, and none of them shows
//! up as an error anywhere.
//!
//! **Signed over raw bytes.** Two JSON documents can mean the same thing and
//! hash differently: key order, whitespace, `1.0` against `1`. A verifier that
//! binds the body to a record and re-serialises to check will reject good
//! deliveries, intermittently, depending on the sender's formatting. The
//! controller therefore takes `@RequestBody byte[]`, which reads like a
//! shortcut and is the whole design.
//!
//! **Compared with `MessageDigest.isEqual`.** `Arrays.equals` returns at the
//! first differing byte, so the time a rejection takes says how much of the
//! signature was right, and a signature can be recovered one byte at a time
//! from that. `isEqual` is documented time-constant and the JDK implements it
//! so (`deps/jdk/.../MessageDigest.java:580`).
//!
//! **Timestamp checked in both directions**, with Stripe's five-minute
//! tolerance, and *inside* the signature. Checking only for staleness leaves a
//! far-future timestamp accepted — the same replay window with its sign
//! flipped — and leaving the timestamp out of the signed bytes makes it a
//! header anyone in the middle can rewrite, at which point there is no window
//! at all.

use super::*;

pub(crate) fn webhook_files(slice: &Slice, name: &str) -> crate::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let web: &str = &slice.owned(Layer::Web);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    // `stripe` -> `app.stripe.secret`, `GitHub` -> `app.git-hub.secret`. Derived
    // rather than asked for, so `destroy` can find it and two projects spell it
    // the same way.
    let property = crate::sql::snake_case(name).replace('_', "-");
    let path = crate::sql::snake_case(name).replace('_', "-");
    let verifier_import = crate::generate::import_of(web, pkg, &format!("{name}Verifier"));

    Ok(vec![
        Artifact {
            kind: "webhook verifier",
            path: main.join(format!("{name}Verifier.java")),
            contents: crate::template::render(
                crate::template::template!("spring/webhook_verifier_java.java"),
                &[("pkg", pkg), ("name", name), ("property", &property)],
            ),
        },
        Artifact {
            kind: "webhook endpoint",
            path: crate::generate::main_dir(root, web)
                .join(format!("{name}WebhookController.java")),
            contents: crate::template::render(
                crate::template::template!("spring/webhook_controller_java.java"),
                &[
                    ("web", web),
                    ("name", name),
                    ("verifier_import", &verifier_import),
                    ("path", &path),
                    ("timestamp_header", &format!("X-{name}-Timestamp")),
                    ("signature_header", &format!("X-{name}-Signature")),
                ],
            ),
        },
        Artifact {
            kind: "webhook verifier test",
            path: test.join(format!("{name}VerifierTest.java")),
            contents: crate::template::render(
                crate::template::template!("spring/webhook_verifier_test_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
        },
    ])
}
