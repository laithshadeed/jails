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

use clap::{Command, ValueEnum};
use jails_support::Result;

/// One name the CLI accepts, with whatever else it answers to.
pub struct Name {
    pub name: String,
    pub aliases: Vec<String>,
    pub about: String,
    /// The long flags this name accepts. Empty for kinds and capabilities,
    /// which are argument *values* rather than subcommands.
    pub options: Vec<String>,
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

/// Every subcommand, at every depth, named by the whole path you type.
///
/// It stopped at depth one, which made this output claim a surface it did not
/// describe: `resource field add`, `remove fast-test`, `app apply` and
/// `db console` are all real commands and none of them was listed. That is
/// the same defect the tables in `jails.nvim` had before they were deleted in
/// favour of reading this -- a completer offering half the verbs -- and it
/// also means a message telling a reader to run `jails remove fast-test`
/// cannot be checked against the parser that would accept it.
///
/// A nested entry's name is the path (`remove fast-test`); its aliases are the
/// path with the leaf's alias substituted, so an alias is still a thing you
/// can type rather than a fragment.
pub fn subcommands(command: &Command) -> Vec<Name> {
    let mut out = Vec::new();
    collect_subcommands(command, command, "", &mut out);
    out
}

fn collect_subcommands(root: &Command, parent: &Command, prefix: &str, out: &mut Vec<Name>) {
    for sub in parent.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        // `help` is clap's, not jails': it is on every command at every depth
        // and listing it would treble this output with one word.
        if sub.get_name() == "help" {
            continue;
        }
        let path = format!("{prefix}{}", sub.get_name());
        out.push(Name {
            name: path.clone(),
            // Only *visible* aliases: `clap_complete`'s bash generator cannot
            // see a hidden one either, which is why `visible_alias` is the rule
            // for anything meant to be typed interactively.
            aliases: sub
                .get_visible_aliases()
                .map(|alias| format!("{prefix}{alias}"))
                .collect(),
            about: sub
                .get_about()
                .map(|about| about.to_string())
                .unwrap_or_default(),
            // This subcommand's own flags plus the global ones, because that
            // is the set a completer should offer after typing it.
            options: long_flags(sub)
                .into_iter()
                .chain(global_flags(root))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
        collect_subcommands(root, sub, &format!("{path} "), out);
    }
}

/// Long flags, including the global ones, which is what a completer needs.
pub fn options(command: &Command) -> Vec<String> {
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
            let aliases: Vec<String> = entry
                .aliases
                .iter()
                .map(|a| crate::json::string(a))
                .collect();
            let options: Vec<String> = entry
                .options
                .iter()
                .map(|o| crate::json::string(o))
                .collect();
            format!(
                "    {{\"name\": {}, \"aliases\": [{}], \"about\": {}, \"options\": [{}]}}",
                crate::json::string(&entry.name),
                aliases.join(", "),
                crate::json::string(&entry.about),
                options.join(", ")
            )
        })
        .collect();
    format!("  \"{label}\": [\n{}\n  ]", rows.join(",\n"))
}

/// The CLI is handed in rather than reached for.
///
/// This module used to name the binary's own `Cli` type, which is one layer
/// above it -- fine while everything was one crate, and a cycle the moment the
/// tooling became one. Taking the `clap::Command` as an argument keeps the
/// property that matters: there is still no second list, because what arrives
/// here is the very command that parsed the arguments.
pub fn commands(command: Command, json: bool) -> Result<()> {
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

    let flag_list: Vec<String> = flags.iter().map(|flag| crate::json::string(flag)).collect();
    println!(
        "{{\n  \"schema_version\": 1,\n{},\n{},\n{},\n  \"options\": [{}]\n}}",
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
}
