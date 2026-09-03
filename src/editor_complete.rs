//! What the *model* offers at a cursor, as opposed to what clap does.
//!
//! `editor complete` walks the clap tree for subcommands and flags, which is
//! every candidate whose answer is the same in every project. The other half
//! is the answers that are not: which entities `--on` can name, which
//! components a field list can filter on, which types a component may take,
//! which markers a field accepts. Those come from the model, and a completer
//! that cannot offer them leaves the reader typing the one thing the tool
//! already knows.
//!
//! **Every path here is best-effort and silent.** A completer runs on every
//! keystroke: a project with no model, a model mid-edit that does not parse,
//! a `--on` naming an entity that does not exist yet -- each returns no
//! candidates rather than a diagnostic, because the reader is in the middle
//! of typing the thing that would fix it.

use crate::project::Project;
use jails_model::AppModel;
use jails_model::field_syntax::java_to_label;

/// One offer, in the shape `editor complete` renders.
pub(crate) struct Candidate {
    pub(crate) value: String,
    /// The protocol's candidate kind: `field`, `type`, `entity` or `marker`.
    pub(crate) kind: &'static str,
    pub(crate) description: Option<String>,
}

/// The candidates the model has for this cursor, or none.
///
/// `command` is the clap subcommand the argv walk resolved, and it is what
/// decides whether the token before the cursor is a flag's *value*: asking
/// clap how many arguments a long flag takes is one lookup against the
/// definition that parses it, where a hand-kept list of value-taking flags
/// would be a second copy that goes stale the first time one is added.
pub(crate) fn candidates(
    command: &clap::Command,
    argv: &[String],
    index: usize,
    prefix: &str,
    project: &Project,
) -> Vec<Candidate> {
    let Some(generate) = argv.iter().take(index).position(is_generate) else {
        return Vec::new();
    };
    let Some(model) = model(project) else {
        return Vec::new();
    };
    if let Some(previous) = index.checked_sub(1).and_then(|at| argv.get(at))
        && let Some(long) = previous.strip_prefix("--")
        && takes_a_value(command, long)
    {
        return match long {
            // The three flags whose value is a declaration the model already
            // has. `--yields` names an event type on an operation and a
            // component on a strategy; both are answered by the same list,
            // because an event is generated from an entity's own name.
            "on" | "via" | "yields" => entities(&model, prefix),
            _ => Vec::new(),
        };
    }
    if prefix.starts_with('-') {
        return Vec::new();
    }
    // **The marker is read off the end, not the start.** A field is one
    // token -- `status:TaskStatus@index` -- so the `@` a reader is typing is
    // usually the third segment of it, and completing only a bare `@` would
    // answer the one position nobody types.
    if let Some(at) = prefix.rfind('@') {
        return markers(&prefix[..at], &prefix[at + 1..]);
    }
    if let Some((component, partial)) = prefix.split_once(':') {
        return types(&model, component, partial);
    }
    if positionals_before(command, argv, generate, index) < 2 {
        // The kind and the name. Neither is the model's to answer: the kind
        // is clap's closed set, and the name is the thing being created.
        return Vec::new();
    }
    components(&model, argv, prefix)
}

fn is_generate(argument: &String) -> bool {
    argument == "g" || argument == "generate"
}

/// The model this project declares, if it declares one that parses.
fn model(project: &Project) -> Option<AppModel> {
    let source = std::fs::read_to_string(project.root().join(".jails").join("model.jdl")).ok()?;
    jails_model::parse_jdl(&source).ok()
}

fn takes_a_value(command: &clap::Command, long: &str) -> bool {
    command
        .get_arguments()
        .find(|argument| argument.get_long() == Some(long))
        .is_some_and(|argument| {
            argument
                .get_num_args()
                .is_none_or(|count| count.takes_values())
        })
}

/// How many positional tokens sit between the `generate` word and the cursor.
///
/// A flag's value is not one, which is what `takes_a_value` is for here: in
/// `g query Recent --on Loan me`, `Loan` is `--on`'s and `me` is the second
/// positional, so the cursor is in the field list.
fn positionals_before(
    command: &clap::Command,
    argv: &[String],
    generate: usize,
    index: usize,
) -> usize {
    let mut seen = 0;
    let mut skip_next = false;
    for argument in argv.iter().take(index).skip(generate + 1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        match argument.strip_prefix("--") {
            Some(long) => skip_next = takes_a_value(command, long),
            None if argument.starts_with('-') => skip_next = false,
            None => seen += 1,
        }
    }
    seen
}

fn entities(model: &AppModel, prefix: &str) -> Vec<Candidate> {
    model
        .entities
        .values()
        .filter(|entity| entity.active && entity.names.java_type.starts_with(prefix))
        .map(|entity| Candidate {
            value: entity.names.java_type.clone(),
            kind: "entity",
            description: Some(format!("{} components", entity.fields.len())),
        })
        .collect()
}

fn markers(head: &str, prefix: &str) -> Vec<Candidate> {
    jails_model::jdl_grammar::FIELD
        .iter()
        .filter(|marker| marker.starts_with(prefix))
        .map(|marker| Candidate {
            value: format!("{head}@{marker}"),
            kind: "marker",
            description: None,
        })
        .collect()
}

/// The types a component may take: the language's own, then the entity and
/// enum names this project declares.
///
/// **Capitalised means the project's, which is the rule and not a
/// convention.** `field_syntax` reads a leading capital as a type the model
/// passes through verbatim, so offering `Loan` beside `uuid` is offering the
/// two halves of one closed answer.
fn types(model: &AppModel, component: &str, prefix: &str) -> Vec<Candidate> {
    let builtins = jails_model::builtin::ALL.iter().map(|(_, row)| {
        (
            row.token.to_string(),
            Some(format!("{} in Java", row.java_boxed)),
        )
    });
    let declared = model
        .entities
        .values()
        .filter(|entity| entity.active)
        .map(|entity| (entity.names.java_type.clone(), Some("declared".to_string())));
    builtins
        .chain(declared)
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, description)| Candidate {
            value: format!("{component}:{name}"),
            kind: "type",
            description,
        })
        .collect()
}

/// The components a field list can name, from the entity the command is about.
///
/// `--on` picks it. With no `--on` every declared entity contributes, which
/// is the right answer for a project with one and a superset for a project
/// with several -- and a superset is what a completer owes a reader who has
/// not yet said which entity they mean.
fn components(model: &AppModel, argv: &[String], prefix: &str) -> Vec<Candidate> {
    let named = argv
        .windows(2)
        .find(|pair| pair[0] == "--on")
        .map(|pair| pair[1].clone());
    let mut offered = std::collections::BTreeMap::new();
    for entity in model.entities.values().filter(|entity| entity.active) {
        if let Some(named) = &named
            && entity.label != java_to_label(named)
            && &entity.names.java_type != named
        {
            continue;
        }
        for field in &entity.fields {
            if !field.names.java_member.starts_with(prefix) {
                continue;
            }
            offered
                .entry(field.names.java_member.clone())
                .or_insert_with(|| {
                    format!("{} on {}", spelling(&field.ty), entity.names.java_type)
                });
        }
    }
    offered
        .into_iter()
        .map(|(member, description)| Candidate {
            value: format!("{member}:"),
            kind: "field",
            description: Some(description),
        })
        .collect()
}

/// A component's type, spelled the way a reader would type it.
fn spelling(ty: &jails_model::TypeRef) -> String {
    match ty {
        jails_model::TypeRef::Builtin(builtin) => jails_model::builtin::ALL
            .iter()
            .find(|(candidate, _)| candidate == builtin)
            .map_or_else(|| "?".to_string(), |(_, row)| row.token.to_string()),
        jails_model::TypeRef::External(name) => name.clone(),
        // `canonical_name` spells a collection the way a reader types it,
        // which is the whole of what this function is for.
        collection => collection.canonical_name(),
    }
}

/// The bash half: what makes `jails g query X st<TAB>` complete `status:`.
///
/// **Appended to `clap_complete`'s script rather than replacing it.** The
/// static script is the right answer for every position whose candidates are
/// the same in every project, and it is generated from the definition that
/// parses the arguments, so hand-writing a replacement would be the second
/// copy this repository spends its gates avoiding. This wrapper runs first,
/// asks the binary what *this* project offers, and falls through to the
/// static script when the answer is empty -- which is every position outside
/// a generator's arguments, and every project with no model.
///
/// Only `g`/`generate` reaches the binary, because that is where the model
/// has something to say and a completer that spawns a process on every TAB
/// is a completer people turn off.
///
/// `_get_comp_words_by_ref -n :` is bash-completion's; without it a word
/// containing a colon is split, and `status:te<TAB>` would send `te` as the
/// whole argument. The fallback is the raw words, which is correct for every
/// token that has no colon in it yet.
pub(crate) const BASH_HOOK: &str = r#"
_jails_from_the_model() {
    local words cword cur
    if declare -F _get_comp_words_by_ref >/dev/null 2>&1; then
        _get_comp_words_by_ref -n : cur words cword
    else
        words=("${COMP_WORDS[@]}")
        cword=$COMP_CWORD
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    [[ ${words[1]} == g || ${words[1]} == generate ]] || return 1
    local answer
    answer=$("${words[0]}" --output json editor complete \
        --arg-index "$((cword - 1))" --byte-offset "${#cur}" \
        -- "${words[@]:1}" 2>/dev/null) || return 1
    local values
    mapfile -t values < <(printf '%s' "$answer" |
        grep -o '"value":"[^"]*"' | sed 's/^"value":"//; s/"$//')
    [[ ${#values[@]} -gt 0 ]] || return 1
    COMPREPLY=("${values[@]}")
    if declare -F __ltrim_colon_completions >/dev/null 2>&1; then
        __ltrim_colon_completions "$cur"
    fi
    return 0
}

_jails_with_the_model() {
    _jails_from_the_model "$@" && return
    _jails "$@"
}

complete -F _jails_with_the_model -o nosort -o bashdefault -o default jails
"#;
