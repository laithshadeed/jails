/// Resolve a built-in template from this package's repository root.
macro_rules! template_here {
    ($name:literal) => {
        jails_java::template_at!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/"), $name)
    };
}

pub(crate) use template_here;
