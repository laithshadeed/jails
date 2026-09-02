//! `template_here!` — this package's wrapper around `jails_java::template_at!`.
//!
//! `CARGO_MANIFEST_DIR` expands at the *call site*, so a template macro cannot
//! bake in its own root: it would resolve to whichever crate invoked it. Each
//! crate that renders templates therefore declares a one-line wrapper naming
//! its own root, and this is the binary's.

/// Resolve a built-in template from this package's repository root.
macro_rules! template_here {
    ($name:literal) => {
        jails_java::template_at!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/"), $name)
    };
}

pub(crate) use template_here;
