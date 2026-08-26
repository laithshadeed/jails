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

fn editor_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
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
fn the_editor_plugin_derives_its_vocabulary_instead_of_copying_it() {
    let source = plugin_source();

    // The four hand-maintained tables are gone. They were the fourth of
    // plan.md §6.1's five copies of "what does the CLI accept", and this test
    // used to *pin* them -- which caught drift after the fact and left the copy
    // in place to drift again. abstract.md §9: a test that polices duplication
    // is a receipt for a decision not yet made. The decision is
    // `jails commands --json`, derived from the same clap definition that
    // parses the arguments.
    for gone in [
        "local KINDS = {",
        "local CAPABILITIES = {",
        "local SUBCOMMANDS = {",
    ] {
        assert!(
            !source.contains(gone),
            "`{gone}` is back in jails.nvim. That table is derived from \
             `jails commands --json` now; reintroducing it reintroduces the drift \
             that once left `:Jails g <Tab>` eight kinds behind the CLI."
        );
    }

    assert!(
        source.contains("'commands', '--json'"),
        "the plugin should read its vocabulary from `jails commands --json`"
    );

    // A completer that errors is worse than one that offers nothing: an older
    // binary, a `jails` off PATH, or a malformed payload must all degrade to an
    // empty menu rather than raising inside a keystroke handler.
    assert!(
        source.contains("pcall(vim.json.decode"),
        "decoding must be guarded -- a completion callback runs on every keystroke"
    );
    assert!(
        source.contains("vim.system({ config.command, 'commands', '--json' }"),
        "completion vocabulary must be loaded asynchronously"
    );
    assert!(
        !source.contains("vim.fn.system(") && !source.contains(":wait()"),
        "completion, diagnostics, and health must never wait on the UI thread"
    );
}

#[test]
fn editor_protocol_supports_structured_plans_receipts_and_watch_state() {
    let source = plugin_source();
    let plugin = editor_file("jails.nvim/plugin/jails.lua");
    for required in [
        "function M.preview(",
        "function M.apply_plan(",
        "function M.watch_toggle(",
        "function M.watch_status(",
        "function M.test_at_cursor(",
        "function M.pick(",
        "function M.health(",
        "'--plan-out'",
        "'--plan-in'",
        "jails.command-result.v2",
        "jails.prepared-report.v1",
        "open_receipt_files(envelope.receipt",
        "JailsWatchStarted",
        "JailsWatchReady",
        "JailsWatchStopped",
    ] {
        assert!(
            source.contains(required),
            "editor protocol lost `{required}`"
        );
    }
    for required in ["'JailsPreview'", "'JailsWatch'", "'JailsHealth'"] {
        assert!(
            plugin.contains(required),
            "editor command lost `{required}`"
        );
    }
    assert!(
        !source.contains("stdout:match('create") && !source.contains("gmatch('create"),
        "created files must come from structured receipt operations"
    );
}

/// The derivation itself: every value the CLI accepts reaches the payload.
#[test]
fn jails_commands_json_carries_every_kind_capability_and_subcommand() {
    let output = std::process::Command::new(common::bin())
        .args(["commands", "--json"])
        .output()
        .expect("jails commands --json");
    assert!(output.status.success());
    let payload = String::from_utf8_lossy(&output.stdout);

    let mut missing: Vec<String> = Vec::new();
    for (label, wanted) in [
        (
            "kind",
            common::scenarios::cli_kinds()
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        (
            "capability",
            common::scenarios::cli_capabilities()
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        ("subcommand", cli_subcommands()),
    ] {
        for value in wanted {
            if !payload.contains(&format!("\"name\": \"{value}\"")) {
                missing.push(format!("{label} `{value}`"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "`jails commands --json` omits {} value(s) the CLI accepts:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

#[test]
fn the_editor_plugin_uses_current_terminal_and_jdtls_apis() {
    let source = plugin_source();

    for required in [
        "vim.fn.jobstart(cmd",
        "term = true",
        "updateBuildConfiguration = 'automatic'",
        "autobuild = { enabled = true }",
        "downloadSources = true",
        "hotCodeReplace = 'auto'",
        "--jvm-arg=-Xmx2G",
        "config.java_bundles",
    ] {
        assert!(source.contains(required), "editor plugin lost `{required}`");
    }
    assert!(
        !source.contains("termopen("),
        "termopen is deprecated on current Neovim; use jobstart(term=true)"
    );
}

#[test]
fn java_buffers_load_project_navigation_and_compiler_support() {
    let source = plugin_source();
    let ftplugin = editor_file("jails.nvim/after/ftplugin/java.lua");
    let compiler = editor_file("jails.nvim/compiler/jails.vim");

    assert!(ftplugin.contains("configure_java_buffer()"), "{ftplugin}");
    for required in [
        "src/main/java",
        "src/test/java",
        "ftplugin_java_source_path",
        "vim.cmd.compiler('jails')",
        "<leader>Jt",
        "<leader>Jc",
        "<leader>Jr",
        "<leader>Jb",
        "<leader>jt",
        "<leader>jc",
    ] {
        assert!(
            source.contains(required),
            "Java editor setup lost `{required}`"
        );
    }
    assert!(
        compiler.contains("CompilerSet makeprg=jails\\ check"),
        "{compiler}"
    );
    assert!(compiler.contains("errorformat"), "{compiler}");
}
