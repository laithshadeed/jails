//! One coherent unit of intended mutation, before anything has been
//! rendered, read or written.
//!
//! ## Why the attribution is a field and not a comment
//!
//! Every change is either *a resource owner's* — an entity or a one-shot wants
//! this — or *maintenance*: `app init`, a rename, an adopt, a format. The
//! distinction decides who is charged with the resulting resource, and it has
//! to be carried rather than inferred, because a maintenance change that got
//! attributed to a nearby owner would make `destroy` delete a file `format`
//! touched.
//!
//! ## Why files, edits and absences are three lists
//!
//! They are three different guarantees. A [`DesiredFile`] is content jails
//! owns end to end. A [`SemanticEdit`] is a keyed change inside a file
//! somebody else owns. An absence is a path that must not exist afterwards;
//! expressing it as "no file entry" would make it indistinguishable from a
//! path this change simply does not mention.

use crate::Result;
use crate::edit::{FactDelta, SemanticEdit, SemanticPrecondition};
use crate::render::{DesiredFile, ManagedPath};
use crate::resource::{DesiredResource, ResourceKey, ResourceOwner};
use jails_support::codec::{Codec, Decoder, Encoder};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Attribution
// ---------------------------------------------------------------------------

/// Who this change is on behalf of.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChangeAttribution {
    Resource(ResourceOwner),
    Maintenance(MaintenanceAttribution),
}

impl Codec for ChangeAttribution {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Resource(owner) => {
                encoder.tag(0);
                owner.encode(encoder)
            }
            Self::Maintenance(kind) => {
                encoder.tag(1);
                encoder.tag(kind.tag());
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Resource(ResourceOwner::decode(decoder)?),
            1 => Self::Maintenance(MaintenanceAttribution::from_tag(decoder.tag()?)?),
            other => Err(format!("unknown change attribution tag {other}"))?,
        })
    }
}

/// The maintenance operations that own no entity.
///
/// They are enumerated rather than lumped together as "not an owner" because
/// the reports name them, and because a maintenance change's resources are
/// charged to nobody — which is only safe when the set of things that can do
/// it is closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceAttribution {
    AppInit,
    Rename,
    AdoptLayout,
    Format,
}

impl MaintenanceAttribution {
    fn tag(self) -> u8 {
        match self {
            Self::AppInit => 0,
            Self::Rename => 1,
            Self::AdoptLayout => 2,
            Self::Format => 4,
        }
    }

    /// Tag 3 was `AdoptLegacy`, and is not reused: a tag reaches a recovered
    /// journal, so a number that meant one thing must not come to mean
    /// another.
    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::AppInit,
            1 => Self::Rename,
            2 => Self::AdoptLayout,
            4 => Self::Format,
            other => Err(format!("unknown maintenance attribution tag {other}"))?,
        })
    }
}

// ---------------------------------------------------------------------------
// The change
// ---------------------------------------------------------------------------

/// One coherent unit of intended mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredChange {
    pub attribution: ChangeAttribution,
    pub resources: Vec<DesiredResource>,
    pub files: Vec<DesiredFile>,
    pub edits: Vec<SemanticEdit>,
    pub absences: Vec<ManagedPath>,
    /// Resources this owner takes responsibility for **if they are already
    /// there**, and never creates.
    ///
    /// A capability that has moved artifacts -- `add json` went from Jackson 2
    /// to Jackson 3 -- has to be able to take the old ones with it when it
    /// goes, or `remove json` leaves the 2.x line beside the 3.x one, which is
    /// the two-majors failure the move was for. The old coordinates are not
    /// *desired*: nothing writes them, and a project that never had them stays
    /// without them. They are claims, so that removal covers them.
    ///
    /// This is deliberately not "assert these are absent". Asserting absence
    /// at install time would delete a dependency the reader may still be using
    /// during a migration they did not ask jails to finish, and it would do it
    /// under the name of adding something.
    pub adopted: Vec<ResourceKey>,
    pub preconditions: Vec<SemanticPrecondition>,
    pub fact_delta: FactDelta,
}

impl DesiredChange {
    pub fn maintenance(kind: MaintenanceAttribution) -> Self {
        Self::for_attribution(ChangeAttribution::Maintenance(kind))
    }

    pub fn owned_by(owner: ResourceOwner) -> Self {
        Self::for_attribution(ChangeAttribution::Resource(owner))
    }

    fn for_attribution(attribution: ChangeAttribution) -> Self {
        Self {
            attribution,
            resources: Vec::new(),
            files: Vec::new(),
            edits: Vec::new(),
            absences: Vec::new(),
            adopted: Vec::new(),
            preconditions: Vec::new(),
            fact_delta: FactDelta::default(),
        }
    }

    /// The checks a plan must pass before anything is prepared.
    ///
    /// Writing a path and declaring it absent in the same change is the one
    /// that has actually happened in practice — a generator that emits a file
    /// while a `destroy` step of the same run removes it — and the result is
    /// order-dependent rather than wrong-looking.
    pub fn validate(&self) -> Result<()> {
        let mut paths = BTreeSet::new();
        for file in &self.files {
            if !paths.insert(&file.path) {
                return Err(format!("{} is written twice in one change", file.path));
            }
        }
        for absence in &self.absences {
            if paths.contains(&absence.path) {
                return Err(format!(
                    "{} is both written and required absent in one change",
                    absence.path
                ));
            }
        }
        let mut keys = BTreeSet::new();
        for resource in &self.resources {
            resource.value.agrees_with(&resource.key)?;
            if !keys.insert(&resource.key) {
                return Err(format!(
                    "resource {:?} is claimed twice in one change",
                    resource.key
                ));
            }
        }
        for key in &self.adopted {
            if keys.contains(key) {
                return Err(format!(
                    "resource {key:?} is both written and adopted in one change; a claim on \
                     something this change also renders is just a claim"
                ));
            }
            if !keys.insert(key) {
                return Err(format!("resource {key:?} is adopted twice in one change"));
            }
        }
        for edit in &self.edits {
            edit.validate()?;
        }
        self.fact_delta.validate()
    }
}
impl Codec for DesiredChange {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.attribution.encode(encoder)?;
        encode_all(encoder, &self.resources, DesiredResource::encode)?;
        encode_all(encoder, &self.files, DesiredFile::encode)?;
        encode_all(encoder, &self.edits, SemanticEdit::encode)?;
        encode_all(encoder, &self.absences, ManagedPath::encode)?;
        encode_all(encoder, &self.adopted, ResourceKey::encode)?;
        encode_all(encoder, &self.preconditions, SemanticPrecondition::encode)?;
        self.fact_delta.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let change = Self {
            attribution: ChangeAttribution::decode(decoder)?,
            resources: decode_all(decoder, DesiredResource::decode)?,
            files: decode_all(decoder, DesiredFile::decode)?,
            edits: decode_all(decoder, SemanticEdit::decode)?,
            absences: decode_all(decoder, ManagedPath::decode)?,
            adopted: decode_all(decoder, ResourceKey::decode)?,
            preconditions: decode_all(decoder, SemanticPrecondition::decode)?,
            fact_delta: FactDelta::decode(decoder)?,
        };
        change.validate()?;
        Ok(change)
    }
}

pub(crate) fn encode_all<T>(
    encoder: &mut Encoder,
    values: &[T],
    mut encode: impl FnMut(&T, &mut Encoder) -> Result<()>,
) -> Result<()> {
    encoder.count(values.len())?;
    for value in values {
        encode(value, encoder)?;
    }
    Ok(())
}

pub(crate) fn decode_all<T>(
    decoder: &mut Decoder<'_>,
    mut decode: impl FnMut(&mut Decoder<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let count = decoder.count()?;
    let mut values = Vec::new();
    for _ in 0..count {
        values.push(decode(decoder)?);
    }
    Ok(values)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::coordinate::MavenCoordinate;
    use crate::entity::{EntityId, IntentId, Recipe};
    use crate::identity::{Name, Package, ProjectPath};
    use crate::render::{DesiredBody, DesiredFile, ManagedPath};
    use crate::resource::ResourceValue;

    pub(crate) fn intent(name: &str) -> IntentId {
        IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::parse("com.example.demo.domain").unwrap(),
        )
    }

    fn owner(name: &str) -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Intent(intent(name)))
    }

    pub(crate) fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    pub(crate) fn dependency_change() -> DesiredChange {
        let coordinate =
            MavenCoordinate::parse("org.springframework.boot", "spring-boot-starter-jdbc").unwrap();
        let key = ResourceKey::MavenDependency(coordinate.clone());
        let mut change = DesiredChange::owned_by(owner("Note"));
        change.resources.push(
            DesiredResource::new(
                key.clone(),
                BTreeSet::from([owner("Note")]),
                ResourceValue::MavenDependency(crate::coordinate::DependencySpec::managed(
                    coordinate.clone(),
                )),
            )
            .unwrap(),
        );
        change.files.push(DesiredFile {
            path: path("src/main/java/com/example/demo/domain/Note.java"),
            body: DesiredBody::Bytes(b"record Note() {}\n".to_vec().into()),
            mode: None,
            resource: Some(ResourceKey::WholeFile(path(
                "src/main/java/com/example/demo/domain/Note.java",
            ))),
            renderer: None,
        });
        change.edits.push(SemanticEdit::MavenDependency {
            key,
            value: crate::coordinate::DependencySpec::managed(coordinate),
        });
        change
    }

    fn round_trip(change: &DesiredChange) -> DesiredChange {
        let mut encoder = Encoder::new();
        change.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = DesiredChange::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        back
    }

    #[test]
    fn a_change_with_a_file_a_resource_and_an_edit_round_trips() {
        let change = dependency_change();
        assert_eq!(round_trip(&change), change);
    }

    /// The order-dependent one. A generator that emits a file while a destroy
    /// step of the same run removes it produces a result that depends on which
    /// half ran last, and nothing about it looks wrong.
    #[test]
    fn writing_and_requiring_absent_the_same_path_is_refused() {
        let mut change = dependency_change();
        change.absences.push(ManagedPath {
            path: path("src/main/java/com/example/demo/domain/Note.java"),
            resource: ResourceKey::WholeFile(path(
                "src/main/java/com/example/demo/domain/Note.java",
            )),
            force: false,
        });
        let error = change.validate().unwrap_err();
        assert!(
            error.contains("both written and required absent"),
            "{error}"
        );
    }

    #[test]
    fn writing_one_path_twice_in_one_change_is_refused() {
        let mut change = dependency_change();
        let duplicate = change.files[0].clone();
        change.files.push(duplicate);
        assert!(change.validate().unwrap_err().contains("written twice"));
    }
}
