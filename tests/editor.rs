//! The editor plugin's vocabulary, pinned to the CLI it completes.
//!
//! `jails.nvim` keeps no completion tables of its own: it reads
//! `jails commands --json`, derived from the clap definition that parses the
//! arguments, so every value the binary accepts reaches the menu. A stale menu
//! is the worst kind of stale, because it looks like the whole set. Aliases
//! and extras are allowed through -- `rule`, `g`, `c` are real things a reader
//! types, and the CLI's long help does not list them.

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

    // No hand-maintained table of the CLI's vocabulary: a copy drifts, and a
    // test that pins the copy catches drift after the fact while leaving it in
    // place. The vocabulary is `jails commands --json`, derived from the same
    // clap definition that parses the arguments.
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

/// The words `syntax/jdl.vim` colours, by highlight group.
///
/// `syn keyword` lines carry a group and then its words; the kebab spellings
/// (`if-match`, `set-null`, `zone-id`) are `syn match` items because keeping
/// `-` out of 'iskeyword' is what lets signed INT literals keep their word
/// boundaries, so they are read off their own matches.
fn syntax_vocabulary() -> Vec<(String, String)> {
    let source = editor_file("jails.nvim/syntax/jdl.vim");
    let mut found = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("syn keyword ") {
            let mut words = rest.split_whitespace();
            let Some(group) = words.next() else { continue };
            for word in words {
                // `contained`, `nextgroup=...` and `skipwhite` are arguments,
                // not vocabulary.
                if word == "contained" || word == "skipwhite" || word.contains('=') {
                    continue;
                }
                found.push((group.to_string(), word.to_string()));
            }
        } else if let Some(rest) = line.strip_prefix("syn match ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let Some(group) = parts.next() else { continue };
            let Some(pattern) = parts.next() else {
                continue;
            };
            // Only the literal kebab words, e.g. `syn match jdlType "\<zone-id\>"`.
            if let Some(word) = pattern
                .strip_prefix("\"\\<")
                .and_then(|p| p.split("\\>").next())
                && word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && word.contains('-')
            {
                found.push((group.to_string(), word.to_string()));
            }
        }
    }
    assert!(
        found.len() > 60,
        "could not read the vocabulary out of syntax/jdl.vim: {found:?}"
    );
    found
}

/// Every JDL word the syntax file colours is one the parser actually matches.
///
/// A syntax file is a hand-written copy of a vocabulary the compiler owns, so
/// it drifts the same way `jails.nvim`'s completion tables once did -- and it
/// drifts invisibly, because a misspelled keyword simply renders in the
/// default colour and looks like an ordinary identifier. The parser's own
/// string literals are the oracle: JDL keywords are matched against them in
/// `crates/jails-model/src/jdl/v1/parser/`, the canonical scalar names live in
/// `builtin.rs`, and the HTTP methods in `unit.rs`.
#[test]
fn the_jdl_syntax_file_colours_only_words_the_parser_knows() {
    let mut oracle = String::new();
    let parser = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-model/src/jdl/v1/parser");
    let mut sources: Vec<_> = std::fs::read_dir(&parser)
        .expect("the JDL parser is tracked in this repository")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    assert!(
        sources.len() >= 5,
        "the JDL parser scan found {} files, so it has lost the code",
        sources.len()
    );
    sources.push(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-model/src/jdl/v1/parser.rs"),
    );
    sources.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-model/src/builtin.rs"));
    sources.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-model/src/unit.rs"));
    for path in &sources {
        oracle.push_str(
            &std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display())),
        );
    }

    // These groups are JDL's own vocabulary. jdlTodo is English, not JDL.
    let vocabulary = [
        "jdlDecl",
        "jdlAppKey",
        "jdlMember",
        "jdlConnective",
        "jdlType",
        "jdlValue",
        "jdlBindSource",
        "jdlAttrScope",
        "jdlBoolean",
        "jdlHttpMethod",
    ];

    let mut unknown = Vec::new();
    for (group, word) in syntax_vocabulary() {
        if !vocabulary.contains(&group.as_str()) {
            continue;
        }
        if !oracle.contains(&format!("\"{word}\"")) {
            unknown.push(format!("{group} {word}"));
        }
    }
    assert!(
        unknown.is_empty(),
        "syntax/jdl.vim colours {unknown:?} as JDL vocabulary, but the parser \
         matches no such word. A syntax file that colours a word the language \
         does not have is worse than one that colours nothing: the editor \
         confirms a misspelling that `jails model check` then refuses."
    );
}

/// `.jdl` buffers get a filetype, which is what makes Copilot work in them.
///
/// github/copilot.vim disables itself in any buffer whose filetype is empty --
/// `s:filetype_defaults` maps '.', its stand-in for no filetype, to 0 -- and
/// Neovim ships no `.jdl` detection, so without this file Copilot is silently
/// off in every model.jdl. The filetype name is also the `languageId`
/// copilot.vim sends to the Copilot LSP.
#[test]
fn jdl_buffers_are_given_a_filetype_so_copilot_stays_enabled() {
    let ftdetect = editor_file("jails.nvim/ftdetect/jdl.lua");
    assert!(
        ftdetect.contains("vim.filetype.add"),
        "ftdetect/jdl.lua no longer registers the filetype, which silently \
         turns Copilot off in every model.jdl: {ftdetect}"
    );
    assert!(
        ftdetect.contains("jdl%s+%d+"),
        "ftdetect/jdl.lua no longer sniffs the `jdl <version>` header. `.jdl` \
         is also JHipster's extension, and claiming theirs is how this plugin \
         breaks an unrelated one: {ftdetect}"
    );

    // The syntax file must not be reachable without the filetype that selects
    // it, and the ftplugin must not grow a second copy of the buffer setup.
    let ftplugin = editor_file("jails.nvim/ftplugin/jdl.lua");
    assert!(
        ftplugin.contains("configure_jdl_buffer"),
        "ftplugin/jdl.lua should delegate to jails.configure_jdl_buffer(), \
         which is where buffer configuration is decided: {ftplugin}"
    );
    assert!(
        editor_file("jails.nvim/syntax/jdl.vim").contains("b:current_syntax"),
        "syntax/jdl.vim must guard and set b:current_syntax"
    );
}
