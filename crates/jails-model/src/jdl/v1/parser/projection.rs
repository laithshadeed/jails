use super::*;

impl Parser<'_> {
    pub(super) fn parse_entity_use(&mut self, entity: &mut EntityDraft) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("use", "JDL0600", "expected an entity use declaration")?;
        let projections = self.projection_list()?;
        if self.at("for") || self.at("except") {
            return Err(self.here(
                "JDL0602",
                "an entity-local use cannot contain `for` or `except`",
                "remove the selector; nesting already selects this entity",
            ));
        }
        self.end_line()?;
        entity.projections.extend(projections);
        self.member(
            &stable_fragment(&entity.name),
            "use",
            None,
            start,
            self.previous_end(),
        );
        Ok(())
    }

    pub(super) fn parse_top_level_use(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("use", "JDL0700", "expected a projection selector")?;
        let projections = self.projection_list()?;
        self.expect(
            "for",
            "JDL0701",
            "a top-level projection use requires `for`",
        )?;
        let selector = if self.consume("*") {
            source::ProjectionSelector::All
        } else {
            source::ProjectionSelector::Named(self.entity_name_list()?)
        };
        let except = if self.consume("except") {
            self.entity_name_list()?
        } else {
            Vec::new()
        };
        self.end_line()?;
        self.projection_rules.push(source::ProjectionRule {
            projections,
            selector,
            except,
        });
        self.declaration("use", None, start, self.previous_end());
        Ok(())
    }

    fn projection_list(&mut self) -> Result<Vec<source::Projection>, Diagnostics> {
        let mut projections = Vec::new();
        loop {
            let kind = self.take_word("projection kind")?;
            if !matches!(
                kind.as_str(),
                "value"
                    | "repo"
                    | "service"
                    | "http"
                    | "dto"
                    | "factory"
                    | "search"
                    | "seed"
                    | "scaffold"
            ) {
                return Err(self.here(
                    "JDL0601",
                    format!("unknown entity projection `{kind}`"),
                    "use value, repo, service, http, dto, factory, search, seed, or scaffold",
                ));
            }
            let mut fields = None;
            let mut path = None;
            if self.consume("(") && !self.consume(")") {
                loop {
                    let argument = self.take_word("projection argument")?;
                    self.expect(":", "JDL0603", "a projection argument needs `name: value`")?;
                    match argument.as_str() {
                        "fields" => {
                            if fields.is_some() {
                                return Err(self.here(
                                    "JDL0604",
                                    "projection argument `fields` is repeated",
                                    "keep one fields argument",
                                ));
                            }
                            let values = self.field_list()?;
                            if values.iter().any(|field| field.contains(' ')) {
                                return Err(self.here(
                                    "JDL0605",
                                    "search fields cannot carry sort directions",
                                    "remove asc/desc from the fields argument",
                                ));
                            }
                            fields = Some(
                                values
                                    .into_iter()
                                    .map(|field| stable_fragment(&field))
                                    .collect(),
                            );
                        }
                        "path" => {
                            if path.is_some() {
                                return Err(self.here(
                                    "JDL0604",
                                    "projection argument `path` is repeated",
                                    "keep one path argument",
                                ));
                            }
                            path = Some(self.take_string("projection route path")?);
                        }
                        other => {
                            return Err(self.here(
                                "JDL0606",
                                format!("unknown projection argument `{other}`"),
                                "use fields on search or path on http/scaffold",
                            ));
                        }
                    }
                    if self.consume(")") {
                        break;
                    }
                    self.expect(
                        ",",
                        "JDL0603",
                        "projection arguments must be comma-separated",
                    )?;
                }
            }
            if fields.is_some() && kind != "search" {
                return Err(self.here(
                    "JDL0607",
                    format!("projection `{kind}` does not accept `fields`"),
                    "use fields only on search",
                ));
            }
            if path.is_some() && !matches!(kind.as_str(), "http" | "scaffold") {
                return Err(self.here(
                    "JDL0607",
                    format!("projection `{kind}` does not accept `path`"),
                    "use path only on http or scaffold",
                ));
            }
            projections.push(source::Projection {
                kind,
                fields: fields.unwrap_or_default(),
                path,
            });
            if !self.consume(",") {
                break;
            }
        }
        Ok(projections)
    }

    fn entity_name_list(&mut self) -> Result<Vec<String>, Diagnostics> {
        let mut names = Vec::new();
        loop {
            names.push(stable_fragment(&self.take_word("entity name")?));
            if !self.consume(",") {
                break;
            }
        }
        Ok(names)
    }
}
