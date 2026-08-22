//! `add sse`: Server-Sent Events, and the four details this design gets wrong.
//!
//! `plan.md` §13.2 names them and each was confirmed in `deps/` rather than
//! from memory: the emitter timeout reaches `AsyncContext.setTimeout` where
//! zero or less means "none" (so `-1L`, never `Long.MAX_VALUE`);
//! `spring.task.scheduling.pool.size` really does default to **1**
//! (`TaskSchedulingProperties.Simple.size`), so one heartbeat blocking on a
//! dead client stalls every other scheduled job; Spring implements no
//! `Last-Event-ID` replay, so emitting an event id would advertise
//! resumability that does not exist; and `ResponseBodyEmitter` holds a
//! `ReentrantLock` rather than `synchronized` in Framework 7, which is what
//! makes sending from a virtual thread viable.
//!
//! A fifth turned up only because the generated test was run: **`complete()`
//! does not fire the completion callbacks unless the emitter is bound to a
//! request.** It sets a flag and forwards to a handler the container installs.
//! That is why the hub exposes `unsubscribe` as real API rather than relying
//! on `onCompletion` alone.
//!
//! Deliberately **topic-agnostic**, the same line `add kafka` draws: a topic is
//! a key the caller chooses, and a capability that invented one would be
//! guessing at a domain.

use super::*;

/// The emitter registry, the stream endpoint, and the one property without
/// which the heartbeat stalls every other scheduled job.
pub(crate) fn sse_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let web: &str = &slice.owned(Layer::Web);
    let name = "Event";
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let hub_import = crate::generate::import_of(web, pkg, &format!("{name}Hub"));
    Change {
        files: vec![
            artifact(
                main.join(format!("{name}Hub.java")),
                sse_hub_java(pkg, name),
            ),
            // Without `@EnableScheduling` the heartbeat annotation is inert and
            // nothing says so: the stream works until a proxy reaps the first
            // idle connection. `kind: "scheduling"` is the shared marker --
            // `generate` skips the file when a job already wrote it, and
            // `tests/agreement.rs` lists it as deliberately kept.
            Artifact {
                kind: "scheduling",
                path: main.join("SchedulingConfig.java"),
                contents: scheduling_config_java(pkg),
            },
            artifact(
                crate::generate::main_dir(root, web).join(format!("{name}StreamController.java")),
                sse_controller_java(web, name, &hub_import),
            ),
            artifact(
                test.join(format!("{name}HubTest.java")),
                sse_hub_test_java(pkg, name),
            ),
        ],
        properties: vec![
            "# The heartbeat runs on this pool, and the default is 1 -- one".to_string(),
            "# heartbeat blocking on a dead client would stall every other".to_string(),
            "# @Scheduled job in the application.".to_string(),
            "spring.task.scheduling.pool.size=4".to_string(),
        ],
        ..Change::default()
    }
}

fn sse_hub_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/sse_hub_java.java"),
        &[("pkg", pkg), ("name", name)],
    )
}

fn sse_controller_java(web: &str, name: &str, hub_import: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/sse_controller_java.java"),
        &[
            ("web", web),
            ("name", name),
            ("hub_import", hub_import),
            ("path", &crate::sql::table_name(name)),
        ],
    )
}

fn sse_hub_test_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/sse_hub_test_java.java"),
        &[("pkg", pkg), ("name", name)],
    )
}
