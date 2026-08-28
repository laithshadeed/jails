use super::{operation_file, resolve_fields, stored_entity};
use crate::CompileError;
use crate::emit_java::{domain_import, primary_key, with_suffix};
use jails_contracts::{ProjectPath, RenderedFile};
use jails_model::{AppModel, BuiltinType, Command, Operation, TypeRef};
use std::collections::BTreeSet;

pub(super) fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    command: &Command,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let target = stored_entity(model, operation, &command.on, "command")?;
    let inputs = resolve_fields(operation, target, &command.fields, "input")?;
    let primary_key = primary_key(target)?;
    let mut imports = BTreeSet::from([
        format!(
            "{}.application.commands.{}",
            model.project.base_package,
            with_suffix(&operation.names.java_type, "Command")
        ),
        domain_import(model, target),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let mut params = String::new();
    for field in target.fields.values() {
        let value = if inputs.iter().any(|input| input.id == field.id) {
            let member = &field.names.java_member;
            if field.required {
                format!("input.{member}()")
            } else {
                format!("input.{member}().orElse(null)")
            }
        } else if field.id == primary_key.id
            && matches!(field.ty, TypeRef::Builtin(BuiltinType::Uuid))
        {
            imports.insert("java.util.UUID".to_string());
            "UUID.randomUUID()".to_string()
        } else if !field.required {
            "(Object) null".to_string()
        } else {
            return Err(CompileError::new(format!(
                "canonical command `{}` cannot construct required field `{}`\n       fix: carry `{}` in the command, or use a UUID primary key that Jails can generate",
                operation.label, field.label, field.label
            )));
        };
        params.push_str(&format!(
            "        statement = statement.param(\"{}\", {value});\n",
            field.names.sql_column
        ));
    }
    let columns = target
        .fields
        .values()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>();
    let column_list = columns.join(", ");
    let values = columns
        .iter()
        .map(|column| format!(":{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let port_type = with_suffix(&operation.names.java_type, "Command");
    let type_name = format!("Jdbc{port_type}");
    let body = format!(
        "@Repository\npublic final class {type_name} implements {port_type} {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public {} execute({port_type}.Input input) {{\n        JdbcClient.StatementSpec statement = jdbc.sql(\"insert into {} ({column_list}) values ({values}) returning {column_list}\");\n{params}        return statement.query({}.class).single();\n    }}\n}}",
        target.names.java_type, target.names.sql_table, target.names.java_type
    );
    operation_file(
        model,
        capability_id,
        operation,
        target,
        &type_name,
        "command",
        "capability-db-command",
        imports,
        body,
    )
}
