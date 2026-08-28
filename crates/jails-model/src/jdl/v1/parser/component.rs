use super::{Parser, one_arg, reject_unknown_attributes, stable_fragment};
use crate::source;
use crate::{ComponentKind, Diagnostics};

impl Parser<'_> {
    pub(super) fn parse_component(&mut self) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("component", "JDL0930", "expected a component declaration")?;
        let raw_kind = self.take_word("component kind")?;
        let kind = ComponentKind::parse(&raw_kind).map_err(|message| {
            self.here(
                "JDL0931",
                message,
                "use one of the closed JDL v1 component kinds",
            )
        })?;
        let name = self.take_word("component name")?;
        let label = stable_fragment(&name);
        let parameters = if self.consume("(") {
            component_parameters(self.parse_parameters(true)?)
        } else {
            Vec::new()
        };
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id"], self)?;
        let id = one_arg(&attributes, "id")?
            .unwrap_or_else(|| format!("cmp_{}_{}", raw_kind.replace('-', "_"), label));
        let mut component = source::Component {
            id,
            name: name.clone(),
            kind,
            parameters,
            on: None,
            yields: None,
            route: None,
            bindings: Vec::new(),
            variants: Vec::new(),
            source: None,
        };

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
                    self.parse_component_member(&mut component)?;
                }
            }
        } else {
            self.end_line()?;
        }

        if self.components.insert(label.clone(), component).is_some() {
            return Err(self.here(
                "JDL0932",
                format!("component `{name}` is declared more than once"),
                "give every component a unique name",
            ));
        }
        self.declaration("component", Some(name), start, self.previous_end());
        Ok(())
    }

    fn parse_component_member(
        &mut self,
        component: &mut source::Component,
    ) -> Result<(), Diagnostics> {
        match self.text() {
            "on" => {
                self.bump();
                let reference = stable_fragment(&self.take_word("component input reference")?);
                set_once(&mut component.on, reference, "on", self)?;
                self.end_line()
            }
            "yields" => {
                self.bump();
                let reference = stable_fragment(&self.take_word("component output reference")?);
                set_once(&mut component.yields, reference, "yields", self)?;
                self.end_line()
            }
            "route" => {
                let route = self.parse_route()?;
                set_once(&mut component.route, route, "route", self)
            }
            "bind" => {
                component.bindings.push(self.parse_binding()?);
                Ok(())
            }
            "variant" => {
                component
                    .variants
                    .push(self.parse_component_variant(&component.id)?);
                Ok(())
            }
            "source" => {
                self.bump();
                let source = self.take_string("component source path")?;
                set_once(&mut component.source, source, "source", self)?;
                self.end_line()
            }
            member => Err(self.here(
                "JDL0933",
                format!("`{member}` is not a component member"),
                "use on, yields, route, bind, variant, or source",
            )),
        }
    }

    fn parse_component_variant(
        &mut self,
        component_id: &str,
    ) -> Result<source::ComponentVariant, Diagnostics> {
        self.expect("variant", "JDL0934", "expected a variant declaration")?;
        let name = self.take_word("variant name")?;
        let parameters = if self.consume("(") {
            component_parameters(self.parse_parameters(true)?)
        } else {
            Vec::new()
        };
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, &["id"], self)?;
        let id = one_arg(&attributes, "id")?
            .unwrap_or_else(|| format!("var_{}_{}", component_id, stable_fragment(&name)));
        self.end_line()?;
        Ok(source::ComponentVariant {
            id,
            name,
            parameters,
        })
    }
}

fn component_parameters(
    parameters: Vec<source::OperationParameter>,
) -> Vec<source::ComponentParameter> {
    parameters
        .into_iter()
        .map(|parameter| {
            let source::ParameterSource::Typed { type_name } = parameter.source else {
                unreachable!("component parameters are parsed in typed-only mode")
            };
            source::ComponentParameter {
                name: parameter.name,
                type_name,
                required: parameter.required,
                constraints: parameter.constraints,
            }
        })
        .collect()
}

fn set_once<T>(
    target: &mut Option<T>,
    value: T,
    member: &str,
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if target.replace(value).is_some() {
        return Err(parser.here(
            "JDL0935",
            format!("component member `{member}` is declared more than once"),
            format!("keep one `{member}` member"),
        ));
    }
    Ok(())
}
