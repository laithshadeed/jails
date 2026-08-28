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
        const CAPS: &[&str] = &[
            "csv",
            "sqlite",
            "json",
            "http",
            "api",
            "actuator",
            "cache",
            "security",
            "cors",
            "sse",
            "mail",
            "redis",
            "observability",
            "kafka",
            "testkit",
            "fake",
            "format",
            "coverage",
            "loadtest",
            "ci",
            "docker",
            "k8s",
            "toxiproxy",
            "fast-test",
        ];
        let start = self.span().start;
        self.expect("cap", "JDL0300", "expected a cap declaration")?;
        let kind = self.take_word("capability kind")?;
        if !CAPS.contains(&kind.as_str()) {
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
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id"], self)?;
        let label = instance.as_ref().map_or_else(
            || stable_fragment(&kind),
            |name| format!("{}_{}", stable_fragment(&kind), stable_fragment(name)),
        );
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("cap_{label}"));
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
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id", "version", "scope"], self)?;
        let label = format!("{}_{}", stable_fragment(&group), stable_fragment(&artifact));
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("dep_{label}"));
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
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id"], self)?;
        let label = stable_fragment(&name);
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("enum_{label}"));
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
                table: None,
                facets: BTreeSet::from([Facet::Enum]),
                values,
                fields: BTreeMap::new(),
                indexes: BTreeMap::new(),
            },
        );
        self.declaration("enum", Some(name), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_entity(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("entity", "JDL0500", "expected an entity declaration")?;
        let name = self.take_word("entity name")?;
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id", "retired"], self)?;
        let label = stable_fragment(&name);
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("ent_{label}"));
        let mut entity = EntityDraft {
            name: name.clone(),
            id,
            active: !has_attribute(&attributes, "retired"),
            table: None,
            facets: BTreeSet::from([Facet::Record]),
            fields: BTreeMap::new(),
            indexes: BTreeMap::new(),
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
                        self.bump();
                        let table = self.take_string("table name")?;
                        set_once(&mut entity.table, table, "table", self)?;
                        self.end_line()?;
                    }
                    "pk" => self.parse_constraint(&mut entity, "pk")?,
                    "unique" => self.parse_constraint(&mut entity, "unique")?,
                    "index" => self.parse_constraint(&mut entity, "index")?,
                    "command" | "query" | "transition" | "event" => {
                        self.parse_operation(Some(&label))?;
                    }
                    "relation" => {
                        return Err(self.here(
                            "JDL0901",
                            format!(
                                "typed `{}` lowering belongs to the next JDL slice",
                                self.text()
                            ),
                            "keep this declaration in legacy JDL until that lowering lands",
                        ));
                    }
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
                table: entity.table,
                facets: entity.facets,
                values: Vec::new(),
                fields: entity.fields,
                indexes: entity.indexes,
            },
        );
        self.declaration("entity", Some(name), start, self.previous_end());
        Ok(())
    }

    fn parse_entity_use(&mut self, entity: &mut EntityDraft) -> Result<(), Diagnostics> {
        self.expect("use", "JDL0600", "expected an entity use declaration")?;
        loop {
            let projection = self.take_word("projection kind")?;
            match projection.as_str() {
                "scaffold" => entity.facets.extend([
                    Facet::Record,
                    Facet::Repository,
                    Facet::Service,
                    Facet::Http,
                ]),
                "record" => {
                    entity.facets.insert(Facet::Record);
                }
                "factory" => {
                    entity.facets.insert(Facet::Factory);
                }
                "dto" => {
                    entity.facets.insert(Facet::Dto);
                }
                "repo" => {
                    entity.facets.insert(Facet::Repository);
                }
                "service" => {
                    entity.facets.insert(Facet::Service);
                }
                "http" => {
                    entity.facets.insert(Facet::Http);
                }
                "events" => {
                    entity.facets.insert(Facet::Events);
                }
                "search" => {
                    entity.facets.insert(Facet::Search);
                }
                "seed" => {
                    entity.facets.insert(Facet::Factory);
                }
                other => {
                    return Err(self.here(
                        "JDL0601",
                        format!("unknown entity projection `{other}`"),
                        "use scaffold, record, repo, service, http, factory, dto, events, search, or seed",
                    ));
                }
            }
            if self.consume("(") {
                self.skip_balanced(")")?;
            }
            if !self.consume(",") {
                break;
            }
        }
        self.end_line()
    }

    fn parse_field(&mut self, entity: &mut EntityDraft) -> Result<(), Diagnostics> {
        let name = self.take_word("field name")?;
        self.expect(":", "JDL0510", "a field declaration needs `:`")?;
        let type_name = self.parse_type_ref()?;
        let required = !self.consume("?");
        let attributes = self.attributes()?;
        reject_unknown_attributes(
            &attributes,
            &["id", "map", "pk", "notBlank", "unique", "index", "length"],
            self,
        )?;
        let field_label = stable_fragment(&name);
        let id = one_arg(&attributes, "id")?
            .unwrap_or_else(|| format!("fld_{}_{}", entity.id, field_label));
        let column = one_arg(&attributes, "map")?;
        let (min_length, max_length) = length(&attributes, self)?;
        self.end_line()?;
        entity.fields.insert(
            field_label,
            source::Field {
                id,
                java_name: Some(name),
                column,
                type_name,
                required,
                non_blank: has_attribute(&attributes, "notBlank"),
                primary_key: has_attribute(&attributes, "pk"),
                unique: has_attribute(&attributes, "unique"),
                indexed: has_attribute(&attributes, "index"),
                min_length,
                max_length,
            },
        );
        Ok(())
    }

    fn parse_constraint(
        &mut self,
        entity: &mut EntityDraft,
        kind: &str,
    ) -> Result<(), Diagnostics> {
        self.bump();
        let columns = self.field_list()?;
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id", "map"], self)?;
        if matches!(kind, "pk" | "unique") && columns.len() == 1 {
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
            return Err(self.here(
                "JDL0902",
                format!("composite `{kind}` is not representable in the current typed model"),
                "use a single-field constraint until composite-key model nodes land",
            ));
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
        self.end_line()
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
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id"], self)?;
        let label = stable_fragment(&target);
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("eject_{label}"));
        self.end_line()?;
        self.ejections
            .insert(label.clone(), source::Ejection { id, target });
        self.declaration("eject", Some(label), start, self.previous_end());
        Ok(())
    }
}
