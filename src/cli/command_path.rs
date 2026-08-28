//! Canonical command identity at the argv edge.

/// Recover the canonical command/subcommand path from argv. Presentation
/// flags may appear before, between, or after those components.
pub(crate) fn command_path_from_env() -> Vec<String> {
    canonical_command_path(std::env::args_os().skip(1))
}

fn canonical_command_path(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
    let mut arguments = arguments.into_iter();
    let mut words = Vec::new();
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "--output" | "--plan-in" | "--plan-out" => {
                let _ = arguments.next();
            }
            "--debug" | "--pretend" | "--dry-run" | "-p" | "--diff" | "--ast" => {}
            value
                if value.starts_with("--output=")
                    || value.starts_with("--plan-in=")
                    || value.starts_with("--plan-out=") => {}
            value if !value.starts_with('-') => words.push(value.to_string()),
            _ => {}
        }
    }
    let Some(first) = words.first() else {
        return Vec::new();
    };
    let first = match first.as_str() {
        "g" => "generate",
        "a" => "add",
        "rm" => "remove",
        "d" => "destroy",
        "dbconsole" => "db",
        "c" => "console",
        other => other,
    };
    let mut path = vec![first.to_string()];
    if let Some(second) = words.get(1).and_then(|word| canonical_child(first, word)) {
        path.push(second.to_string());
        if let Some(third) = words
            .get(2)
            .and_then(|word| canonical_grandchild(first, second, word))
        {
            path.push(third.to_string());
        }
    }
    path
}

fn canonical_child(parent: &str, child: &str) -> Option<&'static str> {
    match (parent, child) {
        ("app", "init" | "plan" | "apply")
        | ("model", "check" | "plan" | "apply" | "eject")
        | ("sql", "check" | "generate" | "explain")
        | ("introspect", "schema" | "query")
        | ("schema", "diff" | "apply")
        | ("editor", "handshake" | "complete" | "symbols" | "diagnostics")
        | ("contract", "emit" | "check")
        | ("resource", "status" | "revive" | "repair" | "field")
        | ("rename", "resource" | "storage") => Some(match child {
            "init" => "init",
            "plan" => "plan",
            "apply" => "apply",
            "eject" => "eject",
            "check" => "check",
            "generate" => "generate",
            "explain" => "explain",
            "schema" => "schema",
            "query" => "query",
            "diff" => "diff",
            "handshake" => "handshake",
            "complete" => "complete",
            "symbols" => "symbols",
            "diagnostics" => "diagnostics",
            "emit" => "emit",
            "status" => "status",
            "revive" => "revive",
            "repair" => "repair",
            "field" => "field",
            "resource" => "resource",
            "storage" => "storage",
            _ => unreachable!(),
        }),
        _ => None,
    }
}

fn canonical_grandchild(parent: &str, child: &str, grandchild: &str) -> Option<&'static str> {
    match (parent, child, grandchild) {
        ("resource", "field", "add" | "rename" | "type" | "nullability" | "drop") => {
            Some(match grandchild {
                "add" => "add",
                "rename" => "rename",
                "type" => "type",
                "nullability" => "nullability",
                "drop" => "drop",
                _ => unreachable!(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_command_path;

    fn path(args: &[&str]) -> Vec<String> {
        canonical_command_path(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn aliases_and_interleaved_presentation_flags_normalize() {
        assert_eq!(
            path(&["--output", "json", "g", "record", "Note", "--pretend"]),
            ["generate"]
        );
        assert_eq!(path(&["rm", "db", "--output=json-v1"]), ["remove"]);
    }

    #[test]
    fn nested_mutation_paths_keep_every_command_component() {
        assert_eq!(
            path(&["resource", "field", "rename", "Note", "title", "name"]),
            ["resource", "field", "rename"]
        );
        assert_eq!(
            path(&["--plan-in=plan.json", "rename", "storage"]),
            ["rename", "storage"]
        );
        assert_eq!(
            path(&["--output", "json", "model", "check"]),
            ["model", "check"]
        );
        assert_eq!(
            path(&["model", "eject", "ent_note", "--pretend"]),
            ["model", "eject"]
        );
    }
}
