//! The `app` block and the top-level statements: `cap`, `dep`, `prop`,
//! `entity` and its members.
//!
//! Every arm follows the same shape — `expect` the keyword, take the name,
//! read the attributes, reject the ones this declaration does not accept —
//! and each refusal carries a `JDL####` code and a fix, so a diagnostic can be
//! looked up rather than only read.
//!
//! **An unknown attribute is an error, never ignored.** `reject_unknown_
//! attributes` is called from every arm for the reason `@primary` is not a
//! silent alias for `@pk`: an attribute quietly dropped produces a model that
//! is missing exactly what the author believed they had asked for, and nothing
//! downstream can tell that from a model where they never asked.
//!
//! Stable IDs are read from `@id(...)` and derived from the label when absent,
//! which is what makes a hand-written model editable without inventing
//! identifiers — and why the derivation must stay stable: it *is* the identity
//! for every declaration written without one.

use super::*;

impl Parser<'_> {
    pub(super) fn parse_app(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("app", "JDL0200", "expected an app declaration")?;
        let name = self.take_word("application type name")?;
        let attributes = self.attributes()?;
        let id = one_arg(&attributes, "id")?;
        self.expect("{", "JDL0200", "an app declaration must open with `{`")?;
        self.end_line()?;
        let mut app = AppDraft {
            name: name.clone(),
            id,
            ..AppDraft::default()
        };
        loop {
            self.skip_layout();
            if self.consume("}") {
                self.end_line()?;
                break;
            }
            let key = self.take_word("app property")?;
            match key.as_str() {
                "pkg" => set_once(
                    &mut app.package,
                    self.take_word("Java package")?,
                    "pkg",
                    self,
                )?,
                "java" => {
                    let value = self.take_integer()?.parse::<u16>().map_err(|_| {
                        self.here(
                            "JDL0206",
                            "Java release is out of range",
                            "use Java 21 or newer",
                        )
                    })?;
                    set_once(&mut app.java, value, "java", self)?;
                }
                "platform" => {
                    let value = self.take_word("platform")?;
                    if !matches!(value.as_str(), "spring" | "plain") {
                        return Err(self.here(
                            "JDL0207",
                            format!("unknown platform `{value}`"),
                            "use `spring` or `plain`",
                        ));
                    }
                    set_once(&mut app.platform, value, "platform", self)?;
                }
                "build" => {
                    let value = self.take_word("build system")?;
                    if !matches!(value.as_str(), "maven" | "gradle") {
                        return Err(self.here(
                            "JDL0208",
                            format!("unknown build system `{value}`"),
                            "use `maven` or `gradle`",
                        ));
                    }
                    set_once(&mut app.build, value, "build", self)?;
                }
                "storage" => {
                    let value = self.take_word("storage")?;
                    if !matches!(value.as_str(), "postgres" | "h2" | "sqlite" | "none") {
                        return Err(self.here(
                            "JDL0209",
                            format!("unknown primary storage `{value}`"),
                            "use `postgres`, `h2`, `sqlite`, or `none`",
                        ));
                    }
                    set_once(&mut app.storage, value, "storage", self)?;
                }
                _ => {
                    return Err(self.here(
                        "JDL0210",
                        format!("unknown app property `{key}`"),
                        "use pkg, java, platform, build, or storage",
                    ));
                }
            }
            self.end_line()?;
        }
        let end = self.previous_end();
        self.declaration("app", Some(name), start, end);
        self.app = Some(app);
        Ok(())
    }

    pub(super) fn parse_cap(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("cap", "JDL0300", "expected a cap declaration")?;
        let kind = self.take_word("capability kind")?;
        if crate::CapabilityKind::declared_in_source(&kind).is_none() {
            return Err(self.here(
                "JDL0301",
                format!("unknown capability kind `{kind}`"),
                "use a capability from the closed JDL v1 registry",
            ));
        }
        let instance = if self.kind() == TokenKind::Word && !self.at("@") {
            Some(self.take_word("capability instance")?)
        } else {
            None
        };
        let label = instance.as_ref().map_or_else(
            || stable_fragment(&kind),
            |name| format!("{}_{}", stable_fragment(&kind), stable_fragment(name)),
        );
        let (_, id) = self.declared(&["id"], || format!("cap_{label}"))?;
        self.end_line()?;
        self.capabilities.insert(
            label.clone(),
            source::Capability {
                id,
                kind,
                name: instance,
                package: None,
            },
        );
        self.declaration("cap", Some(label), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_dep(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("dep", "JDL0310", "expected a dep declaration")?;
        let group = self.take_word("dependency group")?;
        self.expect(":", "JDL0311", "a dependency coordinate needs `:`")?;
        let artifact = self.take_word("dependency artifact")?;
        let label = format!("{}_{}", stable_fragment(&group), stable_fragment(&artifact));
        let (attributes, id) =
            self.declared(&["id", "version", "scope"], || format!("dep_{label}"))?;
        let version = one_arg(&attributes, "version")?;
        let scope = match one_arg(&attributes, "scope")?
            .as_deref()
            .unwrap_or("compile")
        {
            "compile" => DependencyScope::Compile,
            "runtime" => DependencyScope::Runtime,
            "test" => DependencyScope::Test,
            other => {
                return Err(self.here(
                    "JDL0312",
                    format!("unknown dependency scope `{other}`"),
                    "use compile, runtime, or test",
                ));
            }
        };
        self.end_line()?;
        self.dependencies.insert(
            label.clone(),
            source::Dependency {
                id,
                group,
                artifact,
                version,
                scope,
            },
        );
        self.declaration("dep", Some(label), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_prop(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("prop", "JDL0320", "expected a prop declaration")?;
        let key = self.take_word("property key")?;
        self.expect("=", "JDL0321", "a property declaration needs `=`")?;
        let value = self.take_value("property value")?;
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id", "target"], self)?;
        let target = match one_arg(&attributes, "target")?.as_deref().unwrap_or("main") {
            "main" => SettingTarget::Main,
            "test" => SettingTarget::Test,
            other => {
                return Err(self.here(
                    "JDL0322",
                    format!("unknown property target `{other}`"),
                    "use main or test",
                ));
            }
        };
        let label = format!("{}_{}", target.label(), stable_fragment(&key));
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("prop_{label}"));
        self.end_line()?;
        self.settings.insert(
            label.clone(),
            source::Setting {
                id,
                key,
                value,
                target,
            },
        );
        self.declaration("prop", Some(label), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_enum(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("enum", "JDL0400", "expected an enum declaration")?;
        let name = self.take_word("enum name")?;
        let label = stable_fragment(&name);
        let (_, id) = self.declared(&["id"], || format!("enum_{label}"))?;
        let mut values = Vec::new();
        if self.consume("{") {
            if self.consume("}") {
                self.end_line()?;
            } else {
                self.end_line()?;
                loop {
                    self.skip_layout();
                    if self.consume("}") {
                        self.end_line()?;
                        break;
                    }
                    let constant = self.take_word("enum constant")?;
                    let wire = if self.consume("=") {
                        Some(self.take_string("enum wire value")?)
                    } else {
                        None
                    };
                    let value_attributes = self.attributes()?;
                    reject_unknown_attributes(&value_attributes, &["id"], self)?;
                    self.end_line()?;
                    values.push(wire.map_or(constant.clone(), |wire| format!("{constant}={wire}")));
                }
            }
        } else {
            return Err(self.here(
                "JDL0401",
                "an enum declaration needs a block",
                "add `{}` or a brace-delimited constant list",
            ));
        }
        self.entities.insert(
            label.clone(),
            source::Entity {
                id,
                active: true,
                java_name: Some(name.clone()),
                package: None,
                table: None,
                facets: BTreeSet::from([Facet::Enum]),
                values,
                fields: BTreeMap::new(),
                field_order: Vec::new(),
                indexes: BTreeMap::new(),
                constraints: Vec::new(),
                relations: BTreeMap::new(),
                projections: Vec::new(),
            },
        );
        self.declaration("enum", Some(name), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_entity(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("entity", "JDL0500", "expected an entity declaration")?;
        let name = self.take_word("entity name")?;
        let label = stable_fragment(&name);
        let (attributes, id) =
            self.declared(&["id", "retired", "package"], || format!("ent_{label}"))?;
        // **Relative to the base, exactly as a capability's is.** The whole
        // slice goes here instead of the layer packages, and an empty
        // `@package()` means the base itself -- which is how "everything
        // flat" is spelled.
        let package = match has_attribute(&attributes, "package") {
            true => Some(one_arg(&attributes, "package")?.unwrap_or_default()),
            false => None,
        };
        let mut entity = EntityDraft {
            name: name.clone(),
            id,
            active: !has_attribute(&attributes, "retired"),
            package,
            table: None,
            facets: BTreeSet::from([Facet::Record]),
            fields: BTreeMap::new(),
            field_order: Vec::new(),
            indexes: BTreeMap::new(),
            constraints: Vec::new(),
            relations: BTreeMap::new(),
            projections: Vec::new(),
        };
        self.expect("{", "JDL0501", "an entity declaration needs a block")?;
        if self.consume("}") {
            self.end_line()?;
        } else {
            self.end_line()?;
            loop {
                self.skip_layout();
                if self.consume("}") {
                    self.end_line()?;
                    break;
                }
                match self.text() {
                    "use" => self.parse_entity_use(&mut entity)?,
                    "table" => {
                        let start = self.span().start;
                        self.bump();
                        let table = self.take_string("table name")?;
                        set_once(&mut entity.table, table, "table", self)?;
                        self.end_line()?;
                        self.member(
                            &stable_fragment(&entity.name),
                            "table",
                            None,
                            start,
                            self.previous_end(),
                        );
                    }
                    "pk" => self.parse_constraint(&mut entity, "pk")?,
                    "unique" => self.parse_constraint(&mut entity, "unique")?,
                    "index" => self.parse_constraint(&mut entity, "index")?,
                    "command" | "query" | "transition" | "event" => {
                        self.parse_operation(Some(&label))?;
                    }
                    "relation" => self.parse_relation(&mut entity)?,
                    _ => self.parse_field(&mut entity)?,
                }
            }
        }
        self.entities.insert(
            label,
            source::Entity {
                id: entity.id,
                active: entity.active,
                java_name: Some(entity.name),
                package: entity.package,
                table: entity.table,
                facets: entity.facets,
                values: Vec::new(),
                fields: entity.fields,
                field_order: entity.field_order,
                indexes: entity.indexes,
                constraints: entity.constraints,
                relations: entity.relations,
                projections: entity.projections,
            },
        );
        self.declaration("entity", Some(name), start, self.previous_end());
        Ok(())
    }

    fn parse_field(&mut self, entity: &mut EntityDraft) -> Result<(), Diagnostics> {
        let start = self.span().start;
        let name = self.take_word("field name")?;
        self.expect(":", "JDL0510", "a field declaration needs `:`")?;
        let type_name = self.parse_type_ref()?;
        let required = !self.consume("?");
        let attributes = self.attributes()?;
        reject_unknown_attributes(
            &attributes,
            &[
                "id",
                "map",
                "pk",
                "notBlank",
                "unique",
                "index",
                "length",
                "positive",
                "nonnegative",
                "scope",
                "version",
                "default",
                "updated",
            ],
            self,
        )?;
        let field_label = stable_fragment(&name);
        let id = one_arg(&attributes, "id")?
            .unwrap_or_else(|| format!("fld_{}_{}", entity.id, field_label));
        let column = one_arg(&attributes, "map")?;
        let (min_length, max_length) = length(&attributes, self)?;
        let semantics = source::FieldSemantics {
            positive: flag_attribute(&attributes, "positive")?,
            nonnegative: flag_attribute(&attributes, "nonnegative")?,
            scope: field_scope(&attributes, self)?,
            version: flag_attribute(&attributes, "version")?,
            default: one_raw_arg(&attributes, "default")?
                .map(|value| operation::value_from_attribute(&value, self))
                .transpose()?,
            updated: flag_attribute(&attributes, "updated")?,
        };
        self.end_line()?;
        entity.field_order.push(field_label.clone());
        let derived_member = crate::naming::lower_camel_case(&field_label);
        let field_label_text = field_label.clone();
        entity.fields.insert(
            field_label,
            source::Field {
                id,
                // **Pinned only when it is not the derived name.** A field
                // written `user_id` and one written `userId` are the same
                // field -- they share a label -- and both project to the Java
                // component `userId`, because a record component named
                // `user_id` is not Java anybody writes. Recording the written
                // spelling as a pin would make the snake-case half emit
                // `UUID user_id`, and make a convention that is supposed to
                // converge depend on which spelling was typed.
                java_name: (name != derived_member && name != field_label_text)
                    .then(|| name.clone()),
                column,
                type_name,
                required,
                non_blank: flag_attribute(&attributes, "notBlank")?,
                primary_key: flag_attribute(&attributes, "pk")?,
                unique: flag_attribute(&attributes, "unique")?,
                indexed: flag_attribute(&attributes, "index")?,
                min_length,
                max_length,
                semantics,
            },
        );
        self.member(
            &stable_fragment(&entity.name),
            "field",
            Some(name),
            start,
            self.previous_end(),
        );
        Ok(())
    }

    fn parse_constraint(
        &mut self,
        entity: &mut EntityDraft,
        kind: &str,
    ) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.bump();
        let columns = self
            .field_list()?
            .into_iter()
            .map(|column| {
                let mut pieces = column.split_whitespace();
                let field = stable_fragment(pieces.next().unwrap_or_default());
                pieces
                    .next()
                    .map_or(field.clone(), |direction| format!("{field} {direction}"))
            })
            .collect::<Vec<_>>();
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id", "map"], self)?;
        if matches!(kind, "pk" | "unique") && columns.len() == 1 && attributes.is_empty() {
            let field_name = columns[0].split_whitespace().next().unwrap_or_default();
            let field = entity.fields.get_mut(field_name).ok_or_else(|| {
                self.here(
                    "JDL0521",
                    format!("`{field_name}` does not name an earlier field"),
                    "declare the field before its constraint",
                )
            })?;
            if kind == "pk" {
                field.primary_key = true;
            } else {
                field.unique = true;
            }
        } else if kind != "index" {
            let suffix = columns
                .iter()
                .map(|column| stable_fragment(column))
                .collect::<Vec<_>>()
                .join("_");
            let prefix = if kind == "pk" { "pk" } else { "uq" };
            let id = one_arg(&attributes, "id")?
                .unwrap_or_else(|| format!("{prefix}_{}_{}", entity.id, suffix));
            let name = one_arg(&attributes, "map")?;
            entity.constraints.push(source::EntityConstraint {
                id,
                kind: if kind == "pk" {
                    source::ConstraintKind::PrimaryKey
                } else {
                    source::ConstraintKind::Unique
                },
                name,
                fields: columns,
            });
        } else {
            let suffix = columns
                .iter()
                .map(|column| stable_fragment(column))
                .collect::<Vec<_>>()
                .join("_");
            let label = suffix.clone();
            let id = one_arg(&attributes, "id")?
                .unwrap_or_else(|| format!("idx_{}_{}", entity.id, suffix));
            let name = one_arg(&attributes, "map")?;
            entity
                .indexes
                .insert(label, source::Index { id, name, columns });
        }
        self.end_line()?;
        self.member(
            &stable_fragment(&entity.name),
            kind,
            None,
            start,
            self.previous_end(),
        );
        Ok(())
    }

    pub(super) fn parse_eject(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("eject", "JDL0800", "expected an eject declaration")?;
        let target = if self.consume("id") {
            self.expect("(", "JDL0801", "`id` ejection syntax needs `(`")?;
            let target = self.take_word("ejection target id")?;
            self.expect(")", "JDL0801", "`id` ejection syntax needs `)`")?;
            target
        } else {
            self.take_word("implementation boundary")?
        };
        let label = stable_fragment(&target);
        // `@adopted` says the reader wrote this boundary before the model
        // knew it: `jails adopt resource` writes the line, and the compiler
        // excludes the boundary without transferring anything (§16.4).
        let (attributes, id) = self.declared(&["id", "adopted"], || format!("eject_{label}"))?;
        let adopted = flag_attribute(&attributes, "adopted")?;
        self.end_line()?;
        self.ejections.insert(
            label.clone(),
            source::Ejection {
                id,
                target,
                adopted,
            },
        );
        self.declaration("eject", Some(label), start, self.previous_end());
        Ok(())
    }
}
