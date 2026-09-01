//! The identity of an external tool, as a value a plan can carry.
//!
//! ## Why a tool has an identity at all
//!
//! plan.md §R3.3 runs a formatter as part of preparation. A formatter is not
//! a pure function of its input: `spotless:apply` from one Maven, one JDK and
//! one plugin version produces different bytes from another. If the prepared
//! value did not record *which* tool produced its bytes, two machines would
//! prepare "the same" transaction and commit different files, and nothing
//! would say so.
//!
//! So a tool is fingerprinted by everything that can change its output — the
//! executable's bytes, the version it reports, the runner contract, the
//! timeout, what it is allowed to touch and what it may read — and the plan
//! carries that fingerprint. A machine whose formatter differs prepares a
//! visibly different transaction rather than a silently different file.
//!
//! ## Why the argument template is separate from the arguments
//!
//! `OperationToolFingerprint` carries an argv *template*, not the argv. One
//! of its parts is the operation label, which contains the operation id — and
//! the operation id is computed over the identity that contains this template.
//! A literal argv would therefore have to contain its own hash. The template
//! records the shape (`prefix` plus `hex_chars`) and the executor fills it in,
//! which breaks the cycle without losing what the argument *is*.

use crate::Result;
use jails_protocol::identity::{ObjectId, ProjectPath, ToolId};
use jails_support::codec::{Codec, Decoder, Encoder, ordered};
use std::collections::BTreeSet;

/// What makes two invocations of a tool the same invocation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub struct ToolInvocationKey {
    pub tool: ToolId,
    /// The file this invocation is about, when it is about one. A
    /// project-wide formatter has none.
    pub subject: Option<ProjectPath>,
}

/// One offline input a tool is permitted to read.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub struct ToolInput {
    pub path: ProjectPath,
    pub sha256: ObjectId,
}

/// Everything about a tool that can change its output.
///
/// §R3.3 calls this *"the complete execution policy"*. `mutable_scopes` and
/// `offline_inputs` are part of the identity rather than beside it, because a
/// tool allowed to write elsewhere or read something new is not the same tool
/// — and a policy that could change without changing the fingerprint is a
/// policy nothing enforces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIdentityFingerprint {
    pub key: ToolInvocationKey,
    pub executable_sha256: ObjectId,
    pub version_stdout_sha256: ObjectId,
    pub runner_schema: u32,
    pub timeout_ms: u64,
    pub mutable_scopes: BTreeSet<ProjectPath>,
    pub offline_inputs: Vec<ToolInput>,
}

impl ToolIdentityFingerprint {
    /// A timeout of zero would mean "no time at all", which is not a policy
    /// anything can satisfy; an unbounded run is expressed by a large one.
    pub fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 {
            return Err(format!(
                "tool `{}` has a zero timeout; no invocation can satisfy that",
                self.key.tool
            )
            .into());
        }
        let mut previous: Option<&ToolInput> = None;
        for input in &self.offline_inputs {
            ordered(previous, input)?;
            previous = Some(input);
        }
        Ok(())
    }
}
impl Codec for ToolIdentityFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.key.encode(encoder)?;
        self.executable_sha256.encode(encoder)?;
        self.version_stdout_sha256.encode(encoder)?;
        encoder.u32(self.runner_schema);
        encoder.u64(self.timeout_ms);
        encoder.count(self.mutable_scopes.len())?;
        let mut previous: Option<&ProjectPath> = None;
        for scope in &self.mutable_scopes {
            ordered(previous, scope)?;
            previous = Some(scope);
            scope.encode(encoder)?;
        }
        encoder.count(self.offline_inputs.len())?;
        for input in &self.offline_inputs {
            input.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let key = ToolInvocationKey::decode(decoder)?;
        let executable_sha256 = ObjectId::decode(decoder)?;
        let version_stdout_sha256 = ObjectId::decode(decoder)?;
        let runner_schema = decoder.u32()?;
        let timeout_ms = decoder.u64()?;
        let mutable_scopes: BTreeSet<ProjectPath> = decoder.set()?;
        let count = decoder.count()?;
        let mut offline_inputs = Vec::new();
        for _ in 0..count {
            offline_inputs.push(ToolInput::decode(decoder)?);
        }
        let fingerprint = Self {
            key,
            executable_sha256,
            version_stdout_sha256,
            runner_schema,
            timeout_ms,
            mutable_scopes,
            offline_inputs,
        };
        fingerprint.validate()?;
        Ok(fingerprint)
    }
}

/// A tool as it was actually run: its identity plus the exact arguments.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct ToolFingerprint {
    pub identity: ToolIdentityFingerprint,
    pub canonical_args_sha256: ObjectId,
}

/// One tool invocation as preparation will actually make it.
///
/// The fingerprint is the execution policy and the args are what will be
/// passed; `canonical_args_sha256` ties them together. §R3.3: *"there is no
/// duplicate timeout/scope/input authority on `ToolSpec`"* — everything about
/// how the tool may run lives in the identity, so a caller cannot widen a
/// scope or a timeout by building a different spec around the same identity.
///
/// **Nothing constructs one.** `route::format` -- the only tool jails runs --
/// hands `Sandbox::run` an identity and a `Vec<String>` of arguments as two
/// parameters, which is the shape this type exists to make impossible. Closing
/// this crate's API (`pending.md` §7.2) is what said so; the fix is to have
/// that call site build a `ToolSpec`, not to delete the guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSpec {
    pub fingerprint: ToolFingerprint,
    pub args: Vec<String>,
}

impl ToolSpec {
    /// Build one, deriving the argument hash from the arguments themselves so
    /// the two cannot be given separately and disagree.
    pub fn new(identity: ToolIdentityFingerprint, args: Vec<String>) -> Result<Self> {
        identity.validate()?;
        let canonical_args_sha256 = canonical_args(&args)?;
        Ok(Self {
            fingerprint: ToolFingerprint {
                identity,
                canonical_args_sha256,
            },
            args,
        })
    }

    /// A spec whose recorded hash does not cover its own arguments would let a
    /// journal replay a different command than the one the identity names.
    pub fn validate(&self) -> Result<()> {
        self.fingerprint.identity.validate()?;
        if self.fingerprint.canonical_args_sha256 != canonical_args(&self.args)? {
            return Err(format!(
                "{}'s recorded argument hash does not cover its arguments",
                self.fingerprint.identity.key.tool
            )
            .into());
        }
        Ok(())
    }
}

/// `SHA256("JAILS-TOOL-ARGS-1" || encode(args))`.
pub fn canonical_args(args: &[String]) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(args.len())?;
    for arg in args {
        encoder.string(arg)?;
    }
    Ok(ObjectId::from_bytes(jails_support::codec::domain_hash(
        "JAILS-TOOL-ARGS-1",
        &encoder.finish()?,
    )))
}

/// One argv part, as a shape rather than as text.
///
/// `OperationLabel` is the part that cannot be a literal: it contains the
/// operation id, and the operation id is a hash over the value that contains
/// this template. Recording the shape breaks that cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolArgTemplate {
    Literal(String),
    OperationLabel { prefix: String, hex_chars: u8 },
}

impl ToolArgTemplate {
    /// A label of zero hex characters identifies nothing, and one longer than
    /// the digest cannot be filled in.
    pub fn validate(&self) -> Result<()> {
        if let Self::OperationLabel { hex_chars, .. } = self
            && (*hex_chars == 0 || usize::from(*hex_chars) > jails_support::codec::DIGEST_BYTES * 2)
        {
            return Err(format!(
                "an operation label of {hex_chars} hex characters cannot name an operation"
            )
            .into());
        }
        Ok(())
    }
}
impl Codec for ToolArgTemplate {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        match self {
            Self::Literal(text) => {
                encoder.tag(0);
                encoder.string(text)
            }
            Self::OperationLabel { prefix, hex_chars } => {
                encoder.tag(1);
                encoder.string(prefix)?;
                encoder.u32(u32::from(*hex_chars));
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let template = match decoder.tag()? {
            0 => Self::Literal(decoder.string()?),
            1 => Self::OperationLabel {
                prefix: decoder.string()?,
                hex_chars: u8::try_from(decoder.u32()?)
                    .map_err(|_| "operation label length out of range".to_string())?,
            },
            other => Err(format!("unknown tool argument template tag {other}"))?,
        };
        template.validate()?;
        Ok(template)
    }
}

/// A tool as an operation *intends* to run it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationToolFingerprint {
    pub identity: ToolIdentityFingerprint,
    pub args: Vec<ToolArgTemplate>,
}

impl Codec for OperationToolFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.identity.encode(encoder)?;
        encoder.count(self.args.len())?;
        for arg in &self.args {
            arg.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let identity = ToolIdentityFingerprint::decode(decoder)?;
        let count = decoder.count()?;
        let mut args = Vec::new();
        for _ in 0..count {
            args.push(ToolArgTemplate::decode(decoder)?);
        }
        Ok(Self { identity, args })
    }
}

/// The tools an operation intends to run, sorted and unique by key.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperationContextFingerprint {
    pub tools: Vec<OperationToolFingerprint>,
}

/// The schema this fingerprint speaks. Exactly one exists.
pub(crate) const CONTEXT_SCHEMA: u32 = 1;

impl OperationContextFingerprint {
    /// Two entries for one key would let the same tool carry two policies,
    /// and which one applied would depend on iteration order.
    pub fn validate(&self) -> Result<()> {
        let mut previous: Option<&ToolInvocationKey> = None;
        for tool in &self.tools {
            ordered(previous, &tool.identity.key)?;
            previous = Some(&tool.identity.key);
        }
        Ok(())
    }
}
impl Codec for OperationContextFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.u32(CONTEXT_SCHEMA);
        encoder.count(self.tools.len())?;
        for tool in &self.tools {
            tool.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        expect_schema(decoder.u32()?)?;
        let count = decoder.count()?;
        let mut tools = Vec::new();
        for _ in 0..count {
            tools.push(OperationToolFingerprint::decode(decoder)?);
        }
        let context = Self { tools };
        context.validate()?;
        Ok(context)
    }
}

/// The tools a *preparation* actually ran, with their exact arguments.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreparationContextFingerprint {
    pub tools: Vec<ToolFingerprint>,
}

impl PreparationContextFingerprint {
    pub fn validate(&self) -> Result<()> {
        let mut previous: Option<&ToolInvocationKey> = None;
        for tool in &self.tools {
            ordered(previous, &tool.identity.key)?;
            previous = Some(&tool.identity.key);
        }
        Ok(())
    }
}
impl Codec for PreparationContextFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.u32(CONTEXT_SCHEMA);
        encoder.count(self.tools.len())?;
        for tool in &self.tools {
            tool.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        expect_schema(decoder.u32()?)?;
        let count = decoder.count()?;
        let mut tools = Vec::new();
        for _ in 0..count {
            tools.push(ToolFingerprint::decode(decoder)?);
        }
        let context = Self { tools };
        context.validate()?;
        Ok(context)
    }
}

fn expect_schema(schema: u32) -> Result<()> {
    if schema != CONTEXT_SCHEMA {
        return Err(format!("context fingerprint schema {schema} is not {CONTEXT_SCHEMA}").into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jails_support::codec::sha256;

    pub(crate) fn identity(tool: &str) -> ToolIdentityFingerprint {
        ToolIdentityFingerprint {
            key: ToolInvocationKey {
                tool: ToolId::parse(tool).unwrap(),
                subject: None,
            },
            executable_sha256: ObjectId::from_bytes(sha256(tool.as_bytes())),
            version_stdout_sha256: ObjectId::from_bytes(sha256(b"version")),
            runner_schema: 1,
            timeout_ms: 120_000,
            mutable_scopes: BTreeSet::from([ProjectPath::parse("src/main/java").unwrap()]),
            offline_inputs: Vec::new(),
        }
    }

    #[test]
    fn a_tool_identity_round_trips() {
        let one = identity("spotless");
        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(ToolIdentityFingerprint::decode(&mut decoder).unwrap(), one);
        decoder.finish().unwrap();
    }

    /// A policy that can change without changing the fingerprint is a policy
    /// nothing enforces.
    #[test]
    fn widening_what_a_tool_may_write_changes_its_identity() {
        let one = identity("spotless");
        let mut wider = one.clone();
        wider
            .mutable_scopes
            .insert(ProjectPath::parse("src/test/java").unwrap());
        assert_ne!(one, wider);

        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let narrow = encoder.finish().unwrap();
        let mut encoder = Encoder::new();
        wider.encode(&mut encoder).unwrap();
        assert_ne!(narrow, encoder.finish().unwrap());
    }

    #[test]
    fn a_zero_timeout_is_refused() {
        let mut zero = identity("spotless");
        zero.timeout_ms = 0;
        assert!(zero.validate().unwrap_err().contains("zero timeout"));
    }

    /// Two entries for one key would let the same tool carry two policies,
    /// and which one applied would depend on iteration order.
    #[test]
    fn two_policies_for_one_tool_are_refused() {
        let context = OperationContextFingerprint {
            tools: vec![
                OperationToolFingerprint {
                    identity: identity("spotless"),
                    args: Vec::new(),
                },
                OperationToolFingerprint {
                    identity: identity("spotless"),
                    args: Vec::new(),
                },
            ],
        };
        assert!(context.validate().is_err());
    }

    /// The label contains the operation id, and the operation id is a hash
    /// over the value that contains this template — so the template records
    /// the shape rather than the text.
    #[test]
    fn an_operation_label_records_a_shape_that_can_be_filled_in() {
        for bad in [0u8, 65] {
            assert!(
                ToolArgTemplate::OperationLabel {
                    prefix: "jails-".to_string(),
                    hex_chars: bad,
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            ToolArgTemplate::OperationLabel {
                prefix: "jails-".to_string(),
                hex_chars: 12,
            }
            .validate()
            .is_ok()
        );
    }

    /// The hash and the arguments are one fact recorded twice, and this is
    /// the check that keeps them one fact.
    #[test]
    fn a_spec_whose_hash_does_not_cover_its_arguments_is_refused() {
        let mut spec = ToolSpec::new(
            identity("spotless"),
            vec!["spotless:apply".to_string(), "--offline".to_string()],
        )
        .unwrap();
        spec.validate().unwrap();
        spec.args.push("--also-this".to_string());
        assert!(spec.validate().unwrap_err().contains("does not cover"));
    }

    #[test]
    fn argument_hashing_distinguishes_a_split_from_a_join() {
        // "a b" as one argument and as two must not hash the same, or a
        // journal could replay a differently split command line.
        let one = canonical_args(&["a b".to_string()]).unwrap();
        let two = canonical_args(&["a".to_string(), "b".to_string()]).unwrap();
        assert_ne!(one, two);
    }

    #[test]
    fn an_unknown_context_schema_is_refused() {
        let mut encoder = Encoder::new();
        encoder.u32(2);
        encoder.count(0).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(OperationContextFingerprint::decode(&mut decoder).is_err());
    }
}
