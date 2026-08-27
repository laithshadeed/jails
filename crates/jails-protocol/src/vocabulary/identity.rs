//! Validating newtypes: the only place a string becomes a protocol value.
//!
//! ## Why these are types
//!
//! Every one of them is currently a `String` somewhere, and the bugs that
//! motivates are already in this repository's history: a package that was
//! sometimes `Option<&str>` and sometimes `""`, a recorded path that had to be
//! re-derived because nobody could say whether it was project-relative. A
//! newtype answers "has this been checked?" once, at construction, instead of
//! at every use.
//!
//! ## The rules that are not in the RFC
//!
//! plan.md §R1.1 says `Name` and `Package` "validate the Java/package rules
//! once" without spelling them out. They are taken from the Java Language
//! Specification §3.8/§3.9 rather than invented: a Java letter or `_`/`$`
//! first, Java letters and digits after, and never a keyword or the `true`,
//! `false` and `null` literals. `Package` is a dot-separated sequence of the
//! same, and is **allowed to be empty** — `--package ''` puts a generated tree
//! flat in the base package, and CLAUDE.md pins that as a shape that must keep
//! compiling.
//!
//! Restricting the first character to ASCII is deliberate and stricter than
//! the JLS, which permits any Unicode letter. jails generates file *names*
//! from these on filesystems that disagree about Unicode normalisation, so a
//! type whose name is one sequence in the ledger and another on disk is a
//! class of bug not worth accepting for a feature nobody has asked for.

use crate::Result;
use jails_support::codec::{self, Codec, DIGEST_BYTES, Decoder, Encoder};

mod component;
mod literal;
mod route;
mod sql;
pub use component::FieldName;
pub use literal::LiteralValue;
pub use route::RoutePath;
pub use sql::SqlName;

// ---------------------------------------------------------------------------
// Digests
// ---------------------------------------------------------------------------

/// The content address of one stored object. Exactly 32 bytes internally;
/// lowercase 64-hex is a presentation form.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ObjectId([u8; DIGEST_BYTES]);

/// Identifies one logical operation across its attempts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct OperationId([u8; DIGEST_BYTES]);

/// Identifies one prepared transaction; also names its directory.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct TransactionId([u8; DIGEST_BYTES]);

macro_rules! digest_newtype {
    ($name:ident) => {
        impl $name {
            pub fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
                Self(bytes)
            }

            /// Exactly 64 lowercase hex characters. Uppercase refuses, because
            /// parsing then rendering has to be byte-identical.
            pub fn parse_hex(text: &str) -> Result<Self> {
                codec::unhex(text).map(Self)
            }

            pub fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                codec::hex(&self.0)
            }
        }

        impl Codec for $name {
            fn encode(&self, encoder: &mut Encoder) -> Result<()> {
                encoder.digest(&self.0);
                Ok(())
            }

            fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
                decoder.digest().map(Self)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.to_hex())
            }
        }
    };
}

digest_newtype!(ObjectId);
digest_newtype!(OperationId);
digest_newtype!(TransactionId);

// ---------------------------------------------------------------------------
// Java names
// ---------------------------------------------------------------------------

/// A single Java identifier: a type name, a field name, a package segment.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Name(String);

impl Name {
    pub fn parse(text: &str) -> Result<Self> {
        validate_identifier(text, "name")?;
        Ok(Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Codec for Name {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A resolved package. Empty is the base package and is valid: `--package ''`
/// is a supported shape, not a missing value.
///
/// This is why the RFC says "convention resolved; never optional here" —
/// `Option<Package>` at this layer would put "the user did not say" and "the
/// user said flat" into the same slot, which is exactly the ambiguity that
/// made `package: Option<&str>` versus `""` a recurring source of drift.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Package(String);

impl Package {
    /// The base package: everything flat.
    pub fn base() -> Self {
        Self(String::new())
    }

    pub fn parse(text: &str) -> Result<Self> {
        if text.is_empty() {
            return Ok(Self::base());
        }
        if text.starts_with('.') || text.ends_with('.') || text.contains("..") {
            return Err(format!(
                "package `{text}` has an empty segment; segments are separated by single dots"
            )
            .into());
        }
        for segment in text.split('.') {
            validate_identifier(segment, "package segment")?;
        }
        Ok(Self(text.to_string()))
    }

    pub fn is_base(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `com.example` + `domain` -> `com.example.domain`; an empty half is
    /// absorbed rather than producing a leading or trailing dot.
    pub fn join(&self, sub: &Package) -> Package {
        match (self.is_base(), sub.is_base()) {
            (true, _) => sub.clone(),
            (_, true) => self.clone(),
            _ => Package(format!("{}.{}", self.0, sub.0)),
        }
    }
}
impl Codec for Package {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fully qualified Java type: `com.example.demo.domain.Note`.
///
/// Fully qualified always. A bare `Note` would be a name whose meaning depends
/// on an import list nobody recorded, and the ledger has to be able to say
/// which type a reference meant a year later.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct JavaType {
    package: Package,
    name: Name,
}

impl JavaType {
    pub fn new(package: Package, name: Name) -> Self {
        Self { package, name }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let (package, name) = match text.rsplit_once('.') {
            Some((package, name)) => (Package::parse(package)?, name),
            None => (Package::base(), text),
        };
        let name = if package.is_base() && is_java_primitive(name) {
            // Primitive type tokens are JLS keywords, so the ordinary `Name`
            // constructor correctly refuses them as identifiers. A Java type
            // is the one vocabulary where those tokens are valid values.
            Name(name.to_string())
        } else {
            Name::parse(name)?
        };
        Ok(Self { package, name })
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn qualified(&self) -> String {
        if self.package.is_base() {
            self.name.0.clone()
        } else {
            format!("{}.{}", self.package.0, self.name.0)
        }
    }
}

fn is_java_primitive(text: &str) -> bool {
    matches!(
        text,
        "boolean" | "byte" | "char" | "double" | "float" | "int" | "long" | "short" | "void"
    )
}
impl Codec for JavaType {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.qualified())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

impl std::fmt::Display for JavaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.qualified())
    }
}

/// The JLS §3.9 keyword set, plus the three literals §3.10 reserves.
///
/// `_` is a keyword since Java 9, and the contextual keywords (`record`,
/// `sealed`, `permits`, `yield`, …) are deliberately absent: they are legal
/// identifiers, and rejecting `record` would refuse a perfectly good field
/// name for a rule the compiler does not have.
const RESERVED: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "_",
    "true",
    "false",
    "null",
];

/// Every public type in `java.lang`, which the compiler imports into every
/// file whether or not anybody asked.
///
/// A table rather than a refusal: `Name` is "a type name, a field name, a
/// package segment", so it validates *references* as well as declarations, and
/// refusing `String` here would refuse `value:String`. Only a declaration
/// shadows, and only the rendered plan knows there is one --
/// `route::request::naming` asks it.
///
/// `bugs.md` B50: `jails g record String value:string` wrote
/// `public record String(String value)`, whose component is typed as the
/// record rather than as text -- a package member outranks the implicit
/// import. It compiles, its generated test compiles, and the caller who asked
/// for a string field silently got a self-reference. `RESERVED` did not catch
/// it because every Java reserved word is lowercase and a type name is
/// capitalised before the check, so `class`, `int` and `String` all passed.
///
/// **Read off the JDK, not recalled**: the class list of `java.base`'s
/// `java/lang/` filtered to the ones `javap` reports as `public`, on the JDK
/// this project targets. A hand-written subset would be a check that silently
/// stops applying to whatever it omitted, which is the shape this whole
/// refusal exists to remove.
pub const JAVA_LANG: &[&str] = &[
    "AbstractMethodError",
    "Appendable",
    "ArithmeticException",
    "ArrayIndexOutOfBoundsException",
    "ArrayStoreException",
    "AssertionError",
    "AutoCloseable",
    "Boolean",
    "BootstrapMethodError",
    "Byte",
    "Character",
    "CharSequence",
    "Class",
    "ClassCastException",
    "ClassCircularityError",
    "ClassFormatError",
    "ClassLoader",
    "ClassNotFoundException",
    "ClassValue",
    "Cloneable",
    "CloneNotSupportedException",
    "Comparable",
    "Deprecated",
    "Double",
    "Enum",
    "EnumConstantNotPresentException",
    "Error",
    "Exception",
    "ExceptionInInitializerError",
    "Float",
    "FunctionalInterface",
    "IllegalAccessError",
    "IllegalAccessException",
    "IllegalArgumentException",
    "IllegalCallerException",
    "IllegalMonitorStateException",
    "IllegalStateException",
    "IllegalThreadStateException",
    "IncompatibleClassChangeError",
    "IndexOutOfBoundsException",
    "InheritableThreadLocal",
    "InstantiationError",
    "InstantiationException",
    "Integer",
    "InternalError",
    "InterruptedException",
    "IO",
    "Iterable",
    "LayerInstantiationException",
    "LazyConstant",
    "LinkageError",
    "Long",
    "MatchException",
    "Math",
    "Module",
    "ModuleLayer",
    "NegativeArraySizeException",
    "NoClassDefFoundError",
    "NoSuchFieldError",
    "NoSuchFieldException",
    "NoSuchMethodError",
    "NoSuchMethodException",
    "NullPointerException",
    "Number",
    "NumberFormatException",
    "Object",
    "OutOfMemoryError",
    "Override",
    "Package",
    "Process",
    "ProcessBuilder",
    "ProcessHandle",
    "Readable",
    "Record",
    "ReflectiveOperationException",
    "Runnable",
    "Runtime",
    "RuntimeException",
    "RuntimePermission",
    "SafeVarargs",
    "ScopedValue",
    "SecurityException",
    "SecurityManager",
    "Short",
    "StackOverflowError",
    "StackTraceElement",
    "StackWalker",
    "StrictMath",
    "String",
    "StringBuffer",
    "StringBuilder",
    "StringIndexOutOfBoundsException",
    "SuppressWarnings",
    "System",
    "Thread",
    "ThreadDeath",
    "ThreadGroup",
    "ThreadLocal",
    "Throwable",
    "TypeNotPresentException",
    "UnknownError",
    "UnsatisfiedLinkError",
    "UnsupportedClassVersionError",
    "UnsupportedOperationException",
    "VerifyError",
    "VirtualMachineError",
    "Void",
    "WrongThreadException",
];

fn validate_identifier(text: &str, what: &str) -> Result<()> {
    if text.is_empty() {
        return Err(format!("{what} is empty").into());
    }
    let mut chars = text.chars();
    let first = chars.next().expect("checked non-empty above");
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return Err(format!(
            "{what} `{text}` starts with `{first}`; a Java identifier starts with a letter, \
             `_` or `$`"
        )
        .into());
    }
    for character in chars {
        if !(character.is_ascii_alphanumeric() || character == '_' || character == '$') {
            return Err(format!(
                "{what} `{text}` contains `{character}`, which is not valid in a Java identifier"
            )
            .into());
        }
    }
    if RESERVED.contains(&text) {
        return Err(format!("{what} `{text}` is a Java reserved word").into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// A project-relative, `/`-normalised path jails is allowed to name.
///
/// The refusals are the point, and each is a real failure mode:
///
/// - **`..`, absolute paths and platform prefixes** — a recorded path is
///   replayed later by `destroy` and by recovery, so one that escapes the
///   project is a delete outside it.
/// - **`.git` and `target`** — jails must never own version-control state, and
///   `target/` is derived output Maven may delete under it.
/// - **everything under `.jails`** — that is machine state with its own typed
///   representations. A plain path into it would let an ordinary file
///   operation rewrite the ledger, which is precisely what the transaction
///   design exists to prevent.
///
/// The exception allowlist is closed: the human app manifest, the project
/// template override layer, reviewed architecture policy, and checked-in
/// generated SQL/HTTP contracts.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ProjectPath(String);

/// `.jails/app.toml` — the human-owned application manifest.
pub(crate) const APP_MANIFEST: &str = ".jails/app.toml";
/// `.jails/templates` — the human-owned template override layer.
pub(crate) const TEMPLATE_OVERRIDES: &str = ".jails/templates";
/// `.jails/sql-contracts` — checked-in generated SQL evidence.
pub(crate) const SQL_CONTRACTS: &str = ".jails/sql-contracts";
/// `.jails/contracts` — checked-in generated HTTP contracts.
pub(crate) const HTTP_CONTRACTS: &str = ".jails/contracts";
/// `.jails/architecture.toml` — the reviewed project architecture policy.
pub(crate) const ARCHITECTURE_POLICY: &str = ".jails/architecture.toml";

impl ProjectPath {
    pub fn parse(text: &str) -> Result<Self> {
        if text.is_empty() {
            return Err(jails_support::Failure::Told("path is empty".to_string()));
        }
        if text.len() > codec::MAX_PATH_BYTES {
            return Err(format!(
                "path is {} bytes, over the {}-byte limit",
                text.len(),
                codec::MAX_PATH_BYTES
            )
            .into());
        }
        if text.starts_with('/') || text.starts_with('\\') {
            return Err(format!("path `{text}` is absolute; paths are project-relative").into());
        }
        if text.contains('\\') {
            return Err(format!(
                "path `{text}` uses `\\`; the canonical separator is `/` on every platform"
            )
            .into());
        }
        // `C:` and friends. A drive-relative path is not project-relative.
        if text.len() >= 2 && text.as_bytes()[1] == b':' {
            return Err(format!("path `{text}` carries a platform prefix").into());
        }
        let segments: Vec<&str> = text.split('/').collect();
        for segment in &segments {
            match *segment {
                "" => return Err(format!("path `{text}` has an empty segment").into()),
                "." | ".." => {
                    return Err(format!(
                        "path `{text}` contains `{segment}`; a recorded path is replayed by \
                         destroy and by recovery, so it may not be relative to anything"
                    )
                    .into());
                }
                _ => {}
            }
        }
        match segments[0] {
            ".git" => {
                return Err(format!(
                    "path `{text}` is inside `.git`; jails never owns version-control state"
                )
                .into());
            }
            "target" => {
                return Err(format!(
                    "path `{text}` is inside `target`; that is derived output Maven may delete"
                )
                .into());
            }
            ".jails" => {
                let allowed = text == APP_MANIFEST
                    || text == TEMPLATE_OVERRIDES
                    || text.starts_with(&format!("{TEMPLATE_OVERRIDES}/"))
                    || text == SQL_CONTRACTS
                    || text.starts_with(&format!("{SQL_CONTRACTS}/"))
                    || text == HTTP_CONTRACTS
                    || text.starts_with(&format!("{HTTP_CONTRACTS}/"))
                    || text == ARCHITECTURE_POLICY;
                if !allowed {
                    return Err(format!(
                        "path `{text}` is machine state under `.jails`, which has its own typed \
                         representations.\n       fix: only `{APP_MANIFEST}` and \
                         `{TEMPLATE_OVERRIDES}`, `{SQL_CONTRACTS}`, `{HTTP_CONTRACTS}`, and \
                         `{ARCHITECTURE_POLICY}` are reachable this way."
                    )
                    .into());
                }
            }
            _ => {}
        }
        Ok(Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is one of the explicitly reachable paths under `.jails`.
    pub fn is_machine_adjacent(&self) -> bool {
        self.0.starts_with(".jails/")
    }

    pub fn is_sql_contract(&self) -> bool {
        self.0 == SQL_CONTRACTS || self.0.starts_with(&format!("{SQL_CONTRACTS}/"))
    }
}
impl Codec for ProjectPath {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.path(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.path()?)
    }
}

impl std::fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A stored object's address **and its length**.
///
/// plan.md §R3.1: *"`ObjectRef` is an `ObjectId` plus its length; `FileImage`
/// never repeats those facts."* The length travels with the id because every
/// consumer needs it before reading — to check a limit, to size a buffer, to
/// tell a truncated object from a missing one — and a length recorded
/// somewhere else is a length that can disagree with the bytes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ObjectRef {
    pub id: ObjectId,
    pub len: u64,
}

impl ObjectRef {
    pub fn new(id: ObjectId, len: u64) -> Self {
        Self { id, len }
    }
}
impl Codec for ObjectRef {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        encoder.u64(self.len);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            id: ObjectId::decode(decoder)?,
            len: decoder.u64()?,
        })
    }
}

impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}", self.id, self.len)
    }
}

// ---------------------------------------------------------------------------
// Logical identifiers
// ---------------------------------------------------------------------------

/// A non-empty canonical logical identifier that is never an absolute path.
///
/// §R1.1 lists nine of these — `TemplateId`, `ToolId`, `ServiceName`,
/// `MavenId`, `MarkerId`, `VolumeName`, `PropertyKey` and friends. They share
/// one rule, so they share one constructor: a name that is not a location.
macro_rules! logical_id {
    ($name:ident, $what:literal) => {
        #[doc = concat!("A canonical ", $what, ". Non-empty, and never a path.")]
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn parse(text: &str) -> Result<Self> {
                validate_logical(text, $what)?;
                Ok(Self(text.to_string()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Codec for $name {
            fn encode(&self, encoder: &mut Encoder) -> Result<()> {
                encoder.string(&self.0)
            }

            fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
                Self::parse(&decoder.string()?)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

logical_id!(TemplateId, "template id");
logical_id!(TemplateKey, "template key");
logical_id!(ToolId, "tool id");
logical_id!(ServiceName, "compose service name");
logical_id!(MavenId, "Maven coordinate");
logical_id!(MarkerId, "jails marker name");
logical_id!(VolumeName, "compose volume name");
logical_id!(PropertyKey, "property key");
logical_id!(ManagedVersion, "pinned version");

fn validate_logical(text: &str, what: &str) -> Result<()> {
    if text.is_empty() {
        return Err(format!("{what} is empty").into());
    }
    if text.len() > codec::MAX_STRING_BYTES {
        return Err(format!("{what} is over the {}-byte limit", codec::MAX_STRING_BYTES).into());
    }
    if text.starts_with('/') || text.starts_with('\\') {
        return Err(
            format!("{what} `{text}` is an absolute path; it must be a logical name").into(),
        );
    }
    if text.contains("..") {
        return Err(format!("{what} `{text}` contains `..`; it must be a logical name").into());
    }
    if text.trim() != text {
        return Err(format!("{what} `{text}` has leading or trailing whitespace").into());
    }
    if text.chars().any(|c| c.is_control()) {
        return Err(format!("{what} `{text}` contains a control character").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_java_identifier_follows_the_language_spec() {
        for good in ["Note", "note", "_private", "$generated", "Note2", "a"] {
            assert!(Name::parse(good).is_ok(), "{good}");
        }
        for (bad, expect) in [
            ("", "is empty"),
            ("2Note", "starts with"),
            ("my-name", "not valid in a Java identifier"),
            ("my name", "not valid in a Java identifier"),
            ("class", "reserved word"),
            ("null", "reserved word"),
            ("_", "reserved word"),
        ] {
            let error = Name::parse(bad).unwrap_err();
            assert!(error.contains(expect), "{bad}: {error}");
        }
    }

    /// Contextual keywords are legal identifiers. Rejecting `record` would
    /// refuse a good field name for a rule `javac` does not have.
    #[test]
    fn contextual_keywords_are_still_identifiers() {
        for contextual in ["record", "sealed", "permits", "yield", "var"] {
            assert!(Name::parse(contextual).is_ok(), "{contextual}");
        }
    }

    /// `--package ''` is a supported shape, not a missing value.
    #[test]
    fn the_base_package_is_empty_and_valid() {
        let base = Package::parse("").unwrap();
        assert!(base.is_base());
        assert_eq!(base, Package::base());
        assert_eq!(base.as_str(), "");
    }

    #[test]
    fn a_package_is_dot_separated_identifiers() {
        assert_eq!(
            Package::parse("com.example.demo").unwrap().as_str(),
            "com.example.demo"
        );
        for (bad, expect) in [
            (".com", "empty segment"),
            ("com.", "empty segment"),
            ("com..example", "empty segment"),
            ("com.class", "reserved word"),
            ("com.2example", "starts with"),
        ] {
            let error = Package::parse(bad).unwrap_err();
            assert!(error.contains(expect), "{bad}: {error}");
        }
    }

    /// Joining absorbs an empty half rather than producing a stray dot, which
    /// is what keeps `--package ''` compiling.
    #[test]
    fn joining_a_base_package_does_not_produce_a_stray_dot() {
        let base = Package::base();
        let demo = Package::parse("com.example.demo").unwrap();
        let domain = Package::parse("domain").unwrap();

        assert_eq!(demo.join(&domain).as_str(), "com.example.demo.domain");
        assert_eq!(base.join(&domain).as_str(), "domain");
        assert_eq!(demo.join(&base).as_str(), "com.example.demo");
        assert_eq!(base.join(&base).as_str(), "");
    }

    #[test]
    fn a_java_type_is_fully_qualified_and_round_trips() {
        let ty = JavaType::parse("com.example.demo.domain.Note").unwrap();
        assert_eq!(ty.package().as_str(), "com.example.demo.domain");
        assert_eq!(ty.name().as_str(), "Note");
        assert_eq!(ty.qualified(), "com.example.demo.domain.Note");

        // A type in the base package is legitimate, and keeps its spelling.
        let flat = JavaType::parse("Note").unwrap();
        assert!(flat.package().is_base());
        assert_eq!(flat.qualified(), "Note");
    }

    #[test]
    fn java_primitive_types_are_types_even_though_they_are_not_identifiers() {
        for primitive in [
            "boolean", "byte", "char", "double", "float", "int", "long", "short",
        ] {
            let ty = JavaType::parse(primitive).unwrap();
            assert_eq!(ty.to_string(), primitive);
        }
        assert!(Name::parse("int").is_err());
    }

    /// The refusals are the whole point: a recorded path is replayed later by
    /// `destroy` and by recovery.
    #[test]
    fn a_project_path_refuses_everything_that_escapes_the_project() {
        for good in [
            "src/main/java/com/example/demo/Note.java",
            "pom.xml",
            "compose.yaml",
        ] {
            assert!(ProjectPath::parse(good).is_ok(), "{good}");
        }
        for (bad, expect) in [
            ("", "is empty"),
            ("/etc/passwd", "absolute"),
            ("src\\main\\java", "canonical separator"),
            ("C:/windows", "platform prefix"),
            ("../outside", "may not be relative"),
            ("src/../../outside", "may not be relative"),
            ("./src", "may not be relative"),
            ("src//main", "empty segment"),
        ] {
            let error = ProjectPath::parse(bad).unwrap_err();
            assert!(error.contains(expect), "{bad}: {error}");
        }
    }

    #[test]
    fn version_control_and_build_output_are_never_jails_to_own() {
        assert!(
            ProjectPath::parse(".git/config")
                .unwrap_err()
                .contains("version-control")
        );
        assert!(
            ProjectPath::parse("target/classes/A.class")
                .unwrap_err()
                .contains("derived output")
        );
        // A file merely *named* like one is fine: the rule is about the first
        // segment, not a substring.
        assert!(ProjectPath::parse("src/target.txt").is_ok());
        assert!(ProjectPath::parse("docs/.gitignore").is_ok());
    }

    /// Machine state has typed representations. A plain path into `.jails`
    /// would let an ordinary file operation rewrite the ledger.
    #[test]
    fn dot_jails_is_reserved_except_for_explicit_reachable_namespaces() {
        for allowed in [
            ".jails/app.toml",
            ".jails/templates",
            ".jails/templates/generate/command_test.java",
            ".jails/sql-contracts/sample/find-entries.json",
            ".jails/sql-contracts",
        ] {
            assert!(ProjectPath::parse(allowed).is_ok(), "{allowed}");
        }
        for reserved in [
            ".jails/ledger.toml",
            ".jails/objects/ab/cd",
            ".jails/transactions/x/journal",
            ".jails/intents/record-note.files",
            ".jails/models/model-note.files",
            ".jails/files",
            ".jails/version",
            ".jails",
            ".jails/app.toml.bak",
            ".jails/templatesx/a.java",
        ] {
            let error = ProjectPath::parse(reserved).unwrap_err();
            assert!(error.contains("machine state"), "{reserved}: {error}");
        }
    }

    #[test]
    fn a_path_over_the_codec_limit_refuses_before_it_can_be_recorded() {
        let long = format!("src/{}", "a".repeat(codec::MAX_PATH_BYTES));
        assert!(ProjectPath::parse(&long).unwrap_err().contains("over the"));
    }

    #[test]
    fn a_logical_identifier_is_a_name_and_not_a_location() {
        assert!(ServiceName::parse("postgres").is_ok());
        assert!(MavenId::parse("org.postgresql:postgresql").is_ok());
        assert!(PropertyKey::parse("spring.datasource.url").is_ok());
        assert!(ManagedVersion::parse("3.9.16").is_ok());

        for (bad, expect) in [
            ("", "is empty"),
            ("/abs/path", "absolute path"),
            ("../escape", "contains `..`"),
            (" padded", "whitespace"),
            ("padded ", "whitespace"),
            ("with\nnewline", "control character"),
        ] {
            let error = ToolId::parse(bad).unwrap_err();
            assert!(error.contains(expect), "{bad}: {error}");
        }
    }

    /// Every wire decoder calls the same constructor, so a value refused at
    /// the CLI cannot arrive through a recovered journal instead.
    #[test]
    fn decoding_runs_the_same_validation_as_parsing() {
        let mut encoder = Encoder::new();
        encoder.path("../escape").unwrap();
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        let error = ProjectPath::decode(&mut decoder).unwrap_err();
        assert!(error.contains("may not be relative"), "{error}");

        let mut encoder = Encoder::new();
        encoder.string("class").unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(Name::decode(&mut decoder).unwrap_err().contains("reserved"));
    }

    #[test]
    fn every_value_round_trips_through_the_codec() {
        let path = ProjectPath::parse("src/main/java/A.java").unwrap();
        let name = Name::parse("Note").unwrap();
        let package = Package::parse("com.example").unwrap();
        let base = Package::base();
        let ty = JavaType::parse("com.example.Note").unwrap();
        let object = ObjectId::from_bytes(jails_support::codec::sha256(b"x"));

        let mut encoder = Encoder::new();
        path.encode(&mut encoder).unwrap();
        name.encode(&mut encoder).unwrap();
        package.encode(&mut encoder).unwrap();
        base.encode(&mut encoder).unwrap();
        ty.encode(&mut encoder).unwrap();
        object.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(ProjectPath::decode(&mut decoder).unwrap(), path);
        assert_eq!(Name::decode(&mut decoder).unwrap(), name);
        assert_eq!(Package::decode(&mut decoder).unwrap(), package);
        assert_eq!(Package::decode(&mut decoder).unwrap(), base);
        assert_eq!(JavaType::decode(&mut decoder).unwrap(), ty);
        assert_eq!(ObjectId::decode(&mut decoder).unwrap(), object);
        decoder.finish().unwrap();
    }

    #[test]
    fn a_digest_id_is_lowercase_hex_and_round_trips_byte_identically() {
        let id = TransactionId::from_bytes(jails_support::codec::sha256(b"abc"));
        let text = id.to_hex();
        assert_eq!(
            text,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(TransactionId::parse_hex(&text).unwrap(), id);
        assert!(TransactionId::parse_hex(&text.to_uppercase()).is_err());
    }
}
