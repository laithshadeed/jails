//! `g presence`: who is connected, answered the same way on every node.
//!
//! `missing.md` M4b. The Django original tracks admin presence in a
//! module-level dict, under a comment saying it works *because* there is one
//! Daphne process -- the author knew it was wrong and shipped it anyway. That
//! is the failure mode `g auth` and `add sse` exist for: an in-memory presence
//! map is silently correct on one node and silently wrong on two, and nothing
//! reports the difference. It does not throw, it does not warn, it just
//! answers a question about the cluster using one process's memory.
//!
//! So the state is in PostgreSQL, keyed by `(scope, member, node)`, and the
//! generated IT is what keeps it there: two adapters with different node ids,
//! one joins on the first, and the second is asked. An in-memory map fails
//! that test and a shared table passes it, which is the whole difference
//! expressed as something a build can check.
//!
//! **Domain-blind, the same way `g idempotency` is.** A scope is a string the
//! caller picks -- a room, a tenant, a document -- and a member is another.
//! jails does not know what is present in what.
//!
//! Two details that are decisions:
//!
//! - **A row per node, not per member.** A member connected from two nodes is
//!   present until both are gone; keying on `(scope, member)` alone would let
//!   one node's disconnect erase the other's session. `present` therefore
//!   reads `distinct member`.
//! - **A TTL rather than a leave-only protocol.** A process that dies never
//!   sends `leave`, so presence built on explicit departure is permanently
//!   wrong after the first crash. `seen_at` plus a bounded window makes the
//!   stale answer self-correcting, and `heartbeat` is what keeps a live member
//!   inside it.

use super::*;

pub(crate) fn presence_files(slice: &Slice, name: &str) -> Result<Vec<Artifact>> {
    if !slice.project().has_jdbc() {
        return Err(format!(
            "presence {name} needs PostgreSQL/JDBC: presence held in one process's memory is \
             correct on one node and wrong on two, with nothing to say which.\n       fix: run \
             `jails add db` first."
        )
        .into());
    }
    let root: &Path = slice.project().root();
    let app: &str = &slice.placed(Layer::App);
    let adapters: &str = &slice.placed(Layer::Adapters);
    let support = crate::spring::support::TestSupport::resolve(slice.project(), adapters);
    let table = format!("{}_presence", crate::sql::snake_case(name));
    let port = format!("{name}Presence");
    Ok(vec![
        // Without `@EnableScheduling` the sweep annotation is inert and
        // nothing says so -- the table simply grows a row per crashed node
        // forever. `kind: "scheduling"` is the shared marker: `generate` skips
        // the file when a job already wrote it, and `tests/agreement.rs` lists
        // it as deliberately kept.
        Artifact {
            kind: "scheduling",
            path: crate::generate::main_dir(root, &slice.owned(Layer::Jobs))
                .join("SchedulingConfig.java"),
            contents: scheduling_config_java(&slice.owned(Layer::Jobs)),
        },
        Artifact {
            kind: "presence port",
            path: crate::generate::main_dir(root, app).join(format!("{port}.java")),
            contents: crate::template::render(
                crate::template_here!("spring/presence_port_java.java"),
                &[("app", app), ("name", name)],
            ),
        },
        Artifact {
            kind: "presence PostgreSQL adapter",
            path: crate::generate::main_dir(root, adapters).join(format!("Jdbc{port}.java")),
            contents: crate::template::render(
                crate::template_here!("spring/presence_store_java.java"),
                &[
                    ("adapters", adapters),
                    ("name", name),
                    ("table", &table),
                    (
                        "port_import",
                        &crate::generate::import_of(adapters, app, &port),
                    ),
                    ("property", &crate::sql::snake_case(name).replace('_', "-")),
                ],
            ),
        },
        Artifact {
            kind: "presence integration test",
            path: crate::generate::test_dir(root, adapters).join(format!("Jdbc{port}IT.java")),
            contents: crate::template::render(
                crate::template_here!("spring/presence_it_java.java"),
                &[
                    ("adapters", adapters),
                    ("name", name),
                    ("table", &table),
                    ("container_import", &support.import),
                    ("container_annotation", &support.annotation),
                    ("disabled_import", support.disabled_import),
                    ("annotation", support.disabled),
                ],
            ),
        },
        Artifact {
            kind: "presence migration",
            path: crate::generate::migration_file(slice.project(), &format!("create_{table}"))?,
            contents: presence_migration(&table),
        },
    ])
}

/// The table, and the index the sweep needs.
///
/// `seen_at` is not null and there is no `left_at`: a departure is a delete,
/// so a row exists only while somebody is there. That is what makes `present`
/// a single predicate rather than a join against a history.
fn presence_migration(table: &str) -> String {
    format!(
        "-- Presence, shared: one row per (scope, member, node) while that node\n\
         -- believes the member is connected. A member seen by any node is\n\
         -- present, which is the answer a single process's memory cannot give.\n\
         create table {table} (\n\
        \x20 scope text not null check (length(btrim(scope)) > 0),\n\
        \x20 member text not null check (length(btrim(member)) > 0),\n\
        \x20 node text not null check (length(btrim(node)) > 0),\n\
        \x20 seen_at timestamptz not null,\n\
        \x20 primary key (scope, member, node)\n\
         );\n\n\
         -- The sweep deletes by age across every scope, and `present` reads one\n\
         -- scope by age. Both are this index.\n\
         create index {table}_seen_at_idx on {table} (seen_at);\n"
    )
}
