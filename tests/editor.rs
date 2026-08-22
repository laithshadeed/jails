//! The editor plugin's completion tables, pinned to the CLI they complete.
//!
//! `jails.nvim` keeps its own hand-maintained `SUBCOMMANDS`, `KINDS` and
//! `CAPABILITIES` lists, because a Lua plugin cannot read a Rust enum. That
//! makes them copy four of the five `plan.md` §6.1 counts, and they drifted
//! exactly as predicted: **eight kinds and three capabilities** were added to
//! the CLI without the Lua moving, so `:Jails g <Tab>` silently offered a
//! stale menu -- the worst kind of stale, because it looks like the whole set.
//!
//! This test does not remove the copy. It makes the copy *checked*: every
//! value the binary accepts must appear in the table that completes it.
//! Aliases and extras are allowed through -- `rule`, `g`, `c` are real things
//! a reader types, and the CLI's long help does not list them.

mod common;

use std::path::Path;

fn plugin_source() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("jails.nvim/lua/jails/init.lua"),
    )
    .expect("the editor plugin is tracked in this repository")
}

/// The quoted strings of `local <name> = { ... }`.
fn lua_table(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("local {name} = {{"))
        .unwrap_or_else(|| panic!("no `local {name}` table in the plugin"));
    let body_start = start + source[start..].find('{').unwrap();
    let end = body_start
        + source[body_start..]
            .find('}')
            .expect("unterminated table");
    let body = &source[body_start..end];
    let mut found = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find(['\'', '"']) {
        let quote = rest.as_bytes()[open] as char;
        let after = &rest[open + 1..];
        let Some(close) = after.find(quote) else {
            break;
        };
        found.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    assert!(!found.is_empty(), "`local {name}` parsed as empty");
    found
}

/// Canonical subcommand names and their aliases, from `jails --help`.
fn cli_subcommands() -> Vec<String> {
    let output = std::process::Command::new(common::bin())
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    let commands = help
        .split_once("Commands:")
        .expect("clap prints a Commands: section")
        .1;
    let mut found = Vec::new();
    for line in commands.lines() {
        if line.starts_with("Options:") {
            break;
        }
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() != 2 {
            continue;
        }
        let Some(name) = trimmed.split_whitespace().next() else {
            continue;
        };
        if name == "help" {
            continue;
        }
        found.push(name.to_string());
    }
    assert!(
        found.len() > 20 && found.iter().any(|c| c == "generate"),
        "could not read the subcommands out of `jails --help`: {found:?}"
    );
    found
}

#[test]
fn the_editor_plugin_completes_every_value_the_cli_accepts() {
    let source = plugin_source();
    let mut missing: Vec<String> = Vec::new();

    for (table, wanted) in [
        (
            "KINDS",
            common::scenarios::cli_kinds().into_iter().collect::<Vec<_>>(),
        ),
        (
            "CAPABILITIES",
            common::scenarios::cli_capabilities()
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        ("SUBCOMMANDS", cli_subcommands()),
    ] {
        let have = lua_table(&source, table);
        for value in wanted {
            if !have.contains(&value) {
                missing.push(format!("{table} is missing `{value}`"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "jails.nvim would not complete {} value(s) the CLI accepts:\n  {}\n\n\
         Add them to jails.nvim/lua/jails/init.lua. The keymaps that drive it live in \
         a third repository (~/code/my-dotfiles), which this project's git history does \
         not track -- so nothing else will tell you.",
        missing.len(),
        missing.join("\n  ")
    );
}
