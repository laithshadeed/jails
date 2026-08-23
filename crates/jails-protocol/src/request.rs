//! What the user asked for, as a value that survives a crash.
//!
//! ## Why a command needs a fingerprint at all
//!
//! When a reconciliation stops half-way with a stored conflict, the next run
//! has to answer one question: *is this the same command?* Re-parsing the
//! project to find out is not available — the project is exactly what is in an
//! uncertain state, and marker-bearing files may be mid-merge. So the command
//! is projected to a canonical syntax value at the CLI edge, before any
//! project-derived default is consulted, and hashed.
//!
//! ## What the projection deliberately excludes
//!
//! plan.md §R3.1: presentation and debug flags, and `--abort-conflict`. Those
//! change how a run *reports*, not what it does, and including them would make
//! `--debug` look like a different command from the one that stalled. Raw
//! argv, secrets and display text never enter it either — which is a rule
//! about the future as much as the present: a secret-bearing option cannot
//! join this projection without an explicit redacted representation and a
//! protocol-version decision.
//!
//! ## Sets sort, sequences do not
//!
//! `jails add db kafka` and `jails add kafka db` are the same request, so
//! capability positions sort. Field and index order is semantic — a record's
//! components have an order, and a composite index on `(a, b)` is not the one
//! on `(b, a)` — so those positions are preserved exactly. Getting this
//! backwards in either direction produces a wrong answer rather than an error:
//! sorting an ordered position silently accepts a different command as the
//! same one.

use crate::Result;
use crate::identity::ObjectId;
use jails_support::codec::{self, Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};

/// The canonical projection of one command line.
///
/// Built from canonical command and option names *after* alias resolution, so
/// two spellings the CLI promises are equivalent produce identical bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CanonicalRequestSyntaxV1 {
    /// The command and subcommand components, without leading dashes.
    pub command_path: Vec<String>,
    /// Validated UTF-8 lexical values. Set-semantic positions arrive sorted;
    /// ordered positions arrive exactly as written.
    pub positionals: Vec<String>,
    /// Only explicitly supplied semantic options, keyed without leading
    /// dashes. Repeated values keep their order unless the option is a set.
    pub options: BTreeMap<String, Vec<String>>,
    /// Only explicitly supplied semantic flags.
    pub flags: BTreeSet<String>,
}

/// The hash of that projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct RequestSyntaxFingerprint(ObjectId);

impl RequestSyntaxFingerprint {
    pub fn object(&self) -> ObjectId {
        self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        ObjectId::decode(decoder).map(Self)
    }
}

impl CanonicalRequestSyntaxV1 {
    /// `SHA256("JAILS-REQUEST-SYNTAX-1" || encode(self))`, exactly.
    pub fn fingerprint(&self) -> Result<RequestSyntaxFingerprint> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(RequestSyntaxFingerprint(ObjectId::from_bytes(
            codec::domain_hash("JAILS-REQUEST-SYNTAX-1", &encoder.finish()?),
        )))
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.command_path.len())?;
        for part in &self.command_path {
            reject_dashes(part)?;
            encoder.string(part)?;
        }
        // A sequence, not a set: order is preserved and duplicates are legal.
        encoder.count(self.positionals.len())?;
        for value in &self.positionals {
            encoder.string(value)?;
        }
        encoder.count(self.options.len())?;
        let mut previous: Option<&String> = None;
        for (key, values) in &self.options {
            ordered(previous, key)?;
            previous = Some(key);
            reject_dashes(key)?;
            encoder.string(key)?;
            encoder.count(values.len())?;
            for value in values {
                encoder.string(value)?;
            }
        }
        encoder.count(self.flags.len())?;
        let mut previous: Option<&String> = None;
        for flag in &self.flags {
            ordered(previous, flag)?;
            previous = Some(flag);
            reject_dashes(flag)?;
            encoder.string(flag)?;
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let mut command_path = Vec::new();
        for _ in 0..decoder.count()? {
            let part = decoder.string()?;
            reject_dashes(&part)?;
            command_path.push(part);
        }
        let mut positionals = Vec::new();
        for _ in 0..decoder.count()? {
            positionals.push(decoder.string()?);
        }
        let mut options = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..decoder.count()? {
            let key = decoder.string()?;
            ordered(previous.as_ref(), &key)?;
            previous = Some(key.clone());
            reject_dashes(&key)?;
            let mut values = Vec::new();
            for _ in 0..decoder.count()? {
                values.push(decoder.string()?);
            }
            options.insert(key, values);
        }
        let mut flags = BTreeSet::new();
        let mut previous: Option<String> = None;
        for _ in 0..decoder.count()? {
            let flag = decoder.string()?;
            ordered(previous.as_ref(), &flag)?;
            previous = Some(flag.clone());
            reject_dashes(&flag)?;
            flags.insert(flag);
        }
        Ok(Self {
            command_path,
            positionals,
            options,
            flags,
        })
    }

    /// Flags and options this projection must never carry.
    ///
    /// They change how a run reports rather than what it does, so including
    /// one would make `--debug` look like a different command from the one
    /// that stalled — and a stored conflict would refuse to recognise its own
    /// rerun.
    pub const EXCLUDED: &'static [&'static str] = &[
        "debug",
        "output",
        "json",
        "quiet",
        "verbose",
        "abort-conflict",
    ];

    /// Whether a flag or option name belongs in the projection at all.
    pub fn is_semantic(name: &str) -> bool {
        !Self::EXCLUDED.contains(&name)
    }
}

/// Keys arrive already stripped, so a leading dash means a caller skipped the
/// canonicalisation step and `--force` and `force` would hash differently.
fn reject_dashes(value: &str) -> Result<()> {
    if value.starts_with('-') {
        return Err(format!(
            "`{value}` still has a leading dash; the canonical projection stores names without \
             one, or `--force` and `force` would be two different commands"
        ));
    }
    if value.is_empty() {
        return Err("a command, option or flag name is empty".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax() -> CanonicalRequestSyntaxV1 {
        CanonicalRequestSyntaxV1 {
            command_path: vec!["add".to_string()],
            positionals: vec!["db".to_string(), "kafka".to_string()],
            options: BTreeMap::from([("package".to_string(), vec!["com.example".to_string()])]),
            flags: BTreeSet::from(["no-start".to_string()]),
        }
    }

    #[test]
    fn a_projection_round_trips_and_its_fingerprint_is_stable() {
        let one = syntax();
        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(CanonicalRequestSyntaxV1::decode(&mut decoder).unwrap(), one);
        decoder.finish().unwrap();

        assert_eq!(one.fingerprint().unwrap(), syntax().fingerprint().unwrap());
    }

    /// `jails add db kafka` and `jails add kafka db` are the same request. The
    /// *caller* sorts a set-semantic position; this test pins that once sorted
    /// they are indistinguishable, which is the property the sort exists for.
    #[test]
    fn a_set_semantic_position_is_the_same_request_in_either_order() {
        let mut written_one_way = syntax();
        written_one_way.positionals = vec!["kafka".to_string(), "db".to_string()];
        written_one_way.positionals.sort();
        assert_eq!(
            written_one_way.fingerprint().unwrap(),
            syntax().fingerprint().unwrap()
        );
    }

    /// Field and index order is semantic, so an ordered position must *not* be
    /// collapsed. Sorting one would silently accept a different command as the
    /// same one — a wrong answer rather than an error.
    #[test]
    fn an_ordered_position_is_not_collapsed() {
        let mut one = syntax();
        one.positionals = vec!["a:string".to_string(), "b:int".to_string()];
        let mut other = syntax();
        other.positionals = vec!["b:int".to_string(), "a:string".to_string()];
        assert_ne!(one.fingerprint().unwrap(), other.fingerprint().unwrap());

        // A repeated value is legal in a sequence, and it changes the request.
        let mut repeated = syntax();
        repeated.positionals = vec!["a:string".to_string(), "a:string".to_string()];
        assert_ne!(repeated.fingerprint().unwrap(), one.fingerprint().unwrap());
    }

    /// `--debug` must not make a rerun look like a different command, or a
    /// stored conflict would refuse to recognise its own resume.
    #[test]
    fn presentation_and_debug_flags_are_not_semantic() {
        for excluded in [
            "debug",
            "output",
            "json",
            "quiet",
            "verbose",
            "abort-conflict",
        ] {
            assert!(
                !CanonicalRequestSyntaxV1::is_semantic(excluded),
                "{excluded} must be excluded"
            );
        }
        for semantic in ["force", "no-start", "package", "name", "manifest"] {
            assert!(
                CanonicalRequestSyntaxV1::is_semantic(semantic),
                "{semantic} must be kept"
            );
        }
    }

    /// An omitted project-derived default stays distinguishable from an
    /// explicit value: the option is simply absent.
    #[test]
    fn an_omitted_option_is_not_the_same_as_an_explicit_one() {
        let mut explicit = syntax();
        explicit
            .options
            .insert("name".to_string(), vec!["Note".to_string()]);
        let omitted = syntax();
        assert_ne!(
            explicit.fingerprint().unwrap(),
            omitted.fingerprint().unwrap()
        );

        // And an explicitly empty value differs from both.
        let mut empty = syntax();
        empty
            .options
            .insert("name".to_string(), vec![String::new()]);
        assert_ne!(
            empty.fingerprint().unwrap(),
            explicit.fingerprint().unwrap()
        );
        assert_ne!(empty.fingerprint().unwrap(), omitted.fingerprint().unwrap());
    }

    /// Names are stored canonically. `--force` and `force` hashing differently
    /// would make an alias look like a different command.
    #[test]
    fn a_name_that_kept_its_dashes_is_refused() {
        let mut leading = syntax();
        leading.flags = BTreeSet::from(["--no-start".to_string()]);
        let error = leading.fingerprint().unwrap_err();
        assert!(error.contains("leading dash"), "{error}");

        let mut empty = syntax();
        empty.command_path = vec![String::new()];
        assert!(empty.fingerprint().is_err());
    }

    #[test]
    fn changing_any_part_changes_the_fingerprint() {
        let base = syntax().fingerprint().unwrap();
        let variants = [
            CanonicalRequestSyntaxV1 {
                command_path: vec!["remove".to_string()],
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                positionals: vec!["db".to_string()],
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                options: BTreeMap::new(),
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                flags: BTreeSet::new(),
                ..syntax()
            },
        ];
        for variant in variants {
            assert_ne!(variant.fingerprint().unwrap(), base);
        }
    }

    /// The domain prefix is fixed by the RFC, so a second implementation has
    /// to reproduce this exact digest for this exact projection.
    #[test]
    fn the_fingerprint_is_the_specified_domain_hash() {
        let one = syntax();
        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(
            one.fingerprint().unwrap().object().as_bytes(),
            &codec::domain_hash("JAILS-REQUEST-SYNTAX-1", &encoded)
        );
    }
}
