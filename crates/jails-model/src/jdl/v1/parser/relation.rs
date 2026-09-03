//! `relation <name> to <Parent> { ... }`.
//!
//! A relation is a stable child of its entity, with its own ID derived from
//! the pair (`rel_<entity>_<label>`) when the author does not name one, and an
//! optional `@map` for the SQL name — the same identity-versus-projection
//! split every other declaration follows.
//!
//! The block may not be empty: a relation with no mappings names a target and
//! says nothing about how the two are joined, which would parse cleanly and
//! give the compiler nothing to emit. Refusing at the syntax is cheaper than a
//! link error about a model that should not have been representable.

use super::*;

impl Parser<'_> {
    pub(super) fn parse_relation(&mut self, entity: &mut EntityDraft) -> Result<(), Diagnostics> {
        let start = self.span().start;
        self.expect("relation", "JDL0550", "expected a relation declaration")?;
        let name = self.take_word("relation name")?;
        self.expect("to", "JDL0551", "a relation needs `to Parent`")?;
        let target = stable_fragment(&self.take_word("relation target")?);
        let label = stable_fragment(&name);
        let (attributes, id) = self.declared(&["id", "map"], || {
            super::identity::relation_id(&entity.id, &label)
        })?;
        let sql_name = one_arg(&attributes, "map")?;
        self.expect("{", "JDL0552", "a relation needs a non-empty block")?;
        self.end_line()?;
        let mut mappings = Vec::new();
        let mut on_delete = None;
        let mut on_update = None;
        loop {
            self.skip_layout();
            if self.consume("}") {
                self.end_line()?;
                break;
            }
            if self.consume("map") {
                let local = self.take_word("local relation field")?;
                self.expect("->", "JDL0553", "a relation mapping needs `->`")?;
                let remote = self.take_word("remote relation field")?;
                self.end_line()?;
                mappings.push(source::RelationMapping { local, remote });
                continue;
            }
            if self.consume("on") {
                let event = self.take_word("relation action event")?;
                let action = match self.take_word("referential action")?.as_str() {
                    "restrict" => source::ReferentialAction::Restrict,
                    "cascade" => source::ReferentialAction::Cascade,
                    "set-null" => source::ReferentialAction::SetNull,
                    other => {
                        return Err(self.here(
                            "JDL0554",
                            format!("unknown referential action `{other}`"),
                            "use restrict, cascade, or set-null",
                        ));
                    }
                };
                let slot = match event.as_str() {
                    "delete" => &mut on_delete,
                    "update" => &mut on_update,
                    other => {
                        return Err(self.here(
                            "JDL0555",
                            format!("unknown relation action event `{other}`"),
                            "use on delete or on update",
                        ));
                    }
                };
                if slot.replace(action).is_some() {
                    return Err(self.here(
                        "JDL0556",
                        format!("`on {event}` appears more than once"),
                        "keep one action for each relation event",
                    ));
                }
                self.end_line()?;
                continue;
            }
            return Err(self.here(
                "JDL0557",
                format!("unknown relation member `{}`", self.text()),
                "use map, on delete, or on update",
            ));
        }
        if mappings.is_empty() {
            return Err(self.here(
                "JDL0558",
                "a relation cannot be empty",
                "add at least one `map local -> remote` member",
            ));
        }
        if entity
            .relations
            .insert(
                label,
                source::Relation {
                    id,
                    name: name.clone(),
                    target,
                    sql_name,
                    mappings,
                    on_delete: on_delete.unwrap_or_default(),
                    on_update: on_update.unwrap_or_default(),
                },
            )
            .is_some()
        {
            return Err(self.here(
                "JDL0559",
                "a relation name is declared more than once in this entity",
                "give every relation a unique lowerCamel name",
            ));
        }
        self.member(
            &stable_fragment(&entity.name),
            "relation",
            Some(name),
            start,
            self.previous_end(),
        );
        Ok(())
    }
}
