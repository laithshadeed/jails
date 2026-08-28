mod cst;
mod format;
mod parser;
mod token;

pub use cst::{DeclarationCst, DocumentCst};
pub use format::format;
pub use token::{Span, Token, TokenKind};

use crate::{AppModel, Diagnostics};

pub(super) fn is_v1(input: &str) -> bool {
    let Ok(tokens) = token::lex(input) else {
        return first_non_comment_line(input)
            .is_some_and(|line| line.split_whitespace().next() == Some("jdl"));
    };
    let mut syntax = tokens.iter().filter(|token| {
        !matches!(
            token.kind,
            TokenKind::Whitespace
                | TokenKind::Comment
                | TokenKind::Newline
                | TokenKind::TriviaNewline
                | TokenKind::Eof
        )
    });
    syntax
        .next()
        .is_some_and(|token| token.text(input) == "jdl")
}

pub fn parse_cst(input: &str) -> Result<DocumentCst, Diagnostics> {
    let tokens = token::lex(input)?;
    Ok(parser::parse(input, tokens)?.cst)
}

pub(super) fn parse(input: &str) -> Result<AppModel, Diagnostics> {
    let tokens = token::lex(input)?;
    let parsed = parser::parse(input, tokens)?;
    crate::linker::link(parsed.source)
}

fn first_non_comment_line(input: &str) -> Option<&str> {
    input.lines().find_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("//")).then_some(line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DependencyScope, Facet, SettingTarget, StableId};

    const CORE: &str = r#"// retained lead comment
jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
dep org.example:widget @id(dep_widget) @version("1.2.3") @scope(test)
prop server.port = 8080 @target(test)

enum Status {
  OPEN
  IN_PROGRESS = "in_progress"
}

entity Task @id(ent_task) {
  use scaffold, factory, dto

  id: uuid @id(fld_task_id) @pk
  title: string @notBlank @length(1..200) @index
  done: boolean?
  index [title desc]
}
"#;

    #[test]
    fn cst_round_trips_every_byte_and_finds_declaration_spans() {
        let cst = parse_cst(CORE).unwrap();
        assert_eq!(cst.reconstruct(), CORE);
        assert_eq!(cst.declarations.len(), 6);
        let task = cst
            .declarations
            .iter()
            .find(|declaration| declaration.name.as_deref() == Some("Task"))
            .unwrap();
        assert!(cst.declaration_text(task).starts_with("entity Task"));
        assert!(
            cst.declaration_text(task)
                .trim_end_matches(['\r', '\n'])
                .ends_with('}')
        );
        let edited = cst
            .replace_declaration(task, "entity WorkItem @id(ent_task) {}\n")
            .unwrap();
        assert!(edited.contains("// retained lead comment\n"));
        assert!(edited.contains("entity WorkItem @id(ent_task) {}\n"));
        assert!(!edited.contains("entity Task @id(ent_task)"));
    }

    #[test]
    fn v1_lowers_directly_to_the_existing_typed_linker_boundary() {
        let model = parse(CORE).unwrap();
        assert_eq!(model.project.id.as_str(), "project_notes");
        assert_eq!(model.project.dialect, "postgresql");
        let task = model
            .entities
            .values()
            .find(|entity| entity.label == "task")
            .unwrap();
        assert!(task.facets.contains(&Facet::Record));
        assert!(task.facets.contains(&Facet::Repository));
        assert!(task.facets.contains(&Facet::Factory));
        assert!(task.facets.contains(&Facet::Dto));
        let title = task
            .fields
            .values()
            .find(|field| field.label == "title")
            .unwrap();
        assert_eq!(title.length.as_ref().unwrap().min, Some(1));
        assert_eq!(title.length.as_ref().unwrap().max, Some(200));
        assert!(title.indexed);
        let status = model
            .entities
            .values()
            .find(|entity| entity.label == "status")
            .unwrap();
        assert_eq!(status.enum_constants[1].wire_value(), "in_progress");
        assert_eq!(
            model.dependencies.values().next().unwrap().scope,
            DependencyScope::Test
        );
        assert_eq!(
            model.settings.values().next().unwrap().target,
            SettingTarget::Test
        );
    }

    #[test]
    fn missing_version_and_unknown_v1_words_have_stable_diagnostics() {
        let missing = parse("app Demo {}\n").unwrap_err();
        assert_eq!(missing.diagnostics[0].code, "JDL0001");
        let unknown = parse(
            "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\nwat ever\n",
        )
        .unwrap_err();
        assert_eq!(unknown.diagnostics[0].code, "JDL0101");

        let unsupported = crate::parse_jdl("jdl 2\n").unwrap_err();
        assert_eq!(unsupported.diagnostics[0].code, "JDL0001");
    }

    #[test]
    fn v1_parser_has_no_toml_frontend_dependency() {
        let parser = include_str!("parser.rs");
        assert!(!parser.contains("parse_toml"));
        assert!(!parser.contains("toml::"));
    }
}
