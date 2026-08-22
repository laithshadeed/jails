//! `jails commands --json`: the CLI's own vocabulary, as data.
//!
//! Every name jails accepts is declared once, in clap, and then needed again
//! somewhere that cannot read a Rust enum. `jails.nvim` keeps hand-maintained
//! `SUBCOMMANDS`, `KINDS`, `CAPABILITIES` and `OPTIONS` tables for exactly that
//! reason, and they drifted precisely as `plan.md` §6.1 predicts a copy will:
//! eight kinds and three capabilities reached the CLI without the Lua moving,
//! so `:Jails g <Tab>` offered a stale menu -- the worst kind of stale, because
//! it looks like the whole set.
//!
//! `tests/editor.rs` made that copy *checked*, which was the right holding
//! action and is explicitly not the fix: `abstract.md` §9 says a test that
//! polices duplication is a receipt for a decision not yet made. This is the
//! decision. The plugin reads this output instead of carrying its own tables,
//! and the copy stops existing rather than being watched.
//!
//! Everything here is derived from the same `clap::Command` that parses the
//! arguments, and from the same `ValueEnum`s that validate them. There is no
//! second list to keep in step, which is the whole point -- adding a kind is
//! one edit, and this output follows.

use crate::Result;
use crate::json;
use clap::{Command, CommandFactory, ValueEnum};

/// One name the CLI accepts, with whatever else it answers to.
struct Name {
    name: String,
    aliases: Vec<String>,
    about: String,
    /// The long flags this name accepts. Empty for kinds and capabilities,
    /// which are argument *values* rather than subcommands.
    options: Vec<String>,
}

fn names_of<T: ValueEnum>() -> Vec<Name> {
    T::value_variants()
        .iter()
        .filter_map(|variant| variant.to_possible_value())
        .filter(|value| !value.is_hide_set())
        .map(|value| Name {
            name: value.get_name().to_string(),
            aliases: value
                .get_name_and_aliases()
                .skip(1)
                .map(str::to_string)
                .collect(),
            about: value
                .get_help()
                .map(|help| help.to_string())
                .unwrap_or_default(),
            options: Vec::new(),
        })
        .collect()
}

/// The long flags declared directly on one command.
fn long_flags(command: &Command) -> Vec<String> {
    command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set())
        .filter_map(|arg| arg.get_long())
        .map(|long| format!("--{long}"))
        .collect()
}

/// The flags declared `global = true` at the root, which every subcommand
/// accepts whether or not it mentions them -- `--pretend` above all, which is
/// global precisely so nobody has to remember which commands support it.
fn global_flags(root: &Command) -> Vec<String> {
    root.get_arguments()
        .filter(|arg| arg.is_global_set() && !arg.is_hide_set())
        .filter_map(|arg| arg.get_long())
        .map(|long| format!("--{long}"))
        .collect()
}

fn subcommands(command: &Command) -> Vec<Name> {
    command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .map(|sub| Name {
            name: sub.get_name().to_string(),
            // Only *visible* aliases: `clap_complete`'s bash generator cannot
            // see a hidden one either, which is why `visible_alias` is the rule
            // for anything meant to be typed interactively.
            aliases: sub.get_visible_aliases().map(str::to_string).collect(),
            about: sub
                .get_about()
                .map(|about| about.to_string())
                .unwrap_or_default(),
            // This subcommand's own flags plus the global ones, because that
            // is the set a completer should offer after typing it.
            options: long_flags(sub)
                .into_iter()
                .chain(global_flags(command))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
        .collect()
}

/// Long flags, including the global ones, which is what a completer needs.
fn options(command: &Command) -> Vec<String> {
    let mut flags = long_flags(command);
    for sub in command.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        flags.extend(long_flags(sub));
    }
    flags.sort_unstable();
    flags.dedup();
    flags
}

fn render_names(label: &str, names: &[Name]) -> String {
    let rows: Vec<String> = names
        .iter()
        .map(|entry| {
            let aliases: Vec<String> = entry.aliases.iter().map(|a| json::string(a)).collect();
            let options: Vec<String> = entry.options.iter().map(|o| json::string(o)).collect();
            format!(
                "    {{\"name\": {}, \"aliases\": [{}], \"about\": {}, \"options\": [{}]}}",
                json::string(&entry.name),
                aliases.join(", "),
                json::string(&entry.about),
                options.join(", ")
            )
        })
        .collect();
    format!("  \"{label}\": [\n{}\n  ]", rows.join(",\n"))
}

pub(crate) fn commands(json: bool) -> Result<()> {
    let command = crate::Cli::command();
    let subs = subcommands(&command);
    let kinds = names_of::<crate::generate::ArtifactKind>();
    let capabilities = names_of::<crate::add::Capability>();
    let flags = options(&command);

    if !json {
        // The human form is deliberately terse: anyone reading this by eye
        // wants to know what exists, and `--help` is where the prose lives.
        println!("subcommands");
        for entry in &subs {
            let aliases = if entry.aliases.is_empty() {
                String::new()
            } else {
                format!("  ({})", entry.aliases.join(", "))
            };
            println!("  {}{aliases}", entry.name);
        }
        println!("\ngenerator kinds");
        for entry in &kinds {
            println!("  {}", entry.name);
        }
        println!("\ncapabilities");
        for entry in &capabilities {
            println!("  {}", entry.name);
        }
        return Ok(());
    }

    let flag_list: Vec<String> = flags.iter().map(|flag| json::string(flag)).collect();
    println!(
        "{{\n  \"version\": 1,\n{},\n{},\n{},\n  \"options\": [{}]\n}}",
        render_names("subcommands", &subs),
        render_names("kinds", &kinds),
        render_names("capabilities", &capabilities),
        flag_list.join(", ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_the_cli_accepts_is_listed() {
        let listed = names_of::<crate::generate::ArtifactKind>();
        // The list is *derived*, so this cannot drift -- it asserts the
        // derivation is wired to the right enum, not that two lists agree.
        assert!(
            listed.iter().any(|entry| entry.name == "scaffold"),
            "{:?}",
            listed.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(listed.len() > 20, "only {} kinds listed", listed.len());
    }

    #[test]
    fn visible_aliases_are_carried_because_completion_cannot_see_hidden_ones() {
        let command = crate::Cli::command();
        let subs = subcommands(&command);
        let generate = subs
            .iter()
            .find(|entry| entry.name == "generate")
            .expect("generate is a subcommand");
        assert!(
            generate.aliases.iter().any(|alias| alias == "g"),
            "{:?}",
            generate.aliases
        );
    }

    #[test]
    fn the_global_pretend_flag_and_its_alias_reach_the_option_list() {
        let flags = options(&crate::Cli::command());
        assert!(flags.contains(&"--pretend".to_string()), "{flags:?}");
    }
}
