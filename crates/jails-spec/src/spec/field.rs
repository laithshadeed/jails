//! The field spec: `name:type[!?]` with optional `@constraint` markers, and
//! everything derived from it on the Java side.
//!
//! **Case is the rule.** A lowercase type is one jails knows and can build a
//! sample of; a capitalised one is a type the project owns, passed through
//! verbatim with no import. `builtin_by_java_name` is the exception that
//! keeps `id:String` working, without which a natural spelling would read as
//! an unknown project type and silently disable the generated test.
//!
//! `Field::java_type` always holds the **inner** type, with `Optionality`
//! carrying the rest; only `component_type` wraps it back into `Optional<..>`.
//! Two representations of one thing is how `fields_from_record` once
//! produced uncompilable code for a record read off disk.
//!
//! See `sql.rs` for the SQL/JDBC projection of the same spec.

use jails_support::Result;

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub java_type: String,
    pub imports: Vec<&'static str>,
    pub optionality: Optionality,
    /// True when the type came from the project rather than the built-in
    /// table, so jails knows the shape of exactly nothing about it.
    pub owned: bool,
    /// A `List` or `Map` component: copied defensively and defaulted to empty
    /// rather than null-checked.
    pub collection: bool,
    /// Facts attached to this field that its Java type cannot express.
    /// Most affect SQL; `@scope` instead marks a request boundary that must
    /// equal the authenticated principal's same-named claim.
    pub constraints: Constraints,
}

/// The table-level facts about a component that the Java type cannot carry.
///
/// A record says a component is a `UUID`; it cannot say that this UUID and
/// that string are together the primary key, or that an amount must be
/// positive, or that lookups come in by customer. Those are real constraints
/// that a generated migration was silently omitting -- so every generated
/// schema got hand-edited immediately, which defeats the point of generating
/// it.
///
/// Deliberately a **closed set**, not arbitrary SQL. `@positive` is a
/// constraint jails can check it is emitting against a numeric column;
/// `@check(whatever you like)` would be a string jails passes through and
/// cannot validate, which is a worse trade than making people write the two
/// exotic constraints by hand.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Constraints {
    /// Part of the primary key. Several components marked `@pk` make it
    /// composite, in the order they were declared.
    pub primary_key: bool,
    pub unique: bool,
    /// Gets its own single-column index. For a composite or ordered one, use
    /// `--index`.
    pub indexed: bool,
    /// HTTP operations carrying this field must authorize it against the
    /// authenticated principal. It deliberately has no SQL effect.
    pub scoped: bool,
    pub check: Option<NumericCheck>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumericCheck {
    /// `check (col > 0)`
    Positive,
    /// `check (col >= 0)`
    NonNegative,
}

impl NumericCheck {
    pub fn predicate(self, column: &str) -> String {
        match self {
            NumericCheck::Positive => format!("{column} > 0"),
            NumericCheck::NonNegative => format!("{column} >= 0"),
        }
    }
}

/// What a `!` or `?` suffix on a field type means.
///
/// Hardcoding one policy is what made `value` reject every blank string,
/// including the description fields where blank is perfectly legal. Every
/// value type in every project has this distinction, so it belongs in the
/// syntax rather than in jails' opinion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Optionality {
    /// `name:string` -- must not be null.
    Required,
    /// `name:string!` -- must not be null, and must not be blank.
    NonBlank,
    /// `name:string?` -- may be null; nothing is checked.
    Nullable,
}

/// One resolved type: how to spell it in Java, and what it needs imported.
pub(crate) struct Resolved {
    java_type: String,
    imports: Vec<&'static str>,
    owned: bool,
    collection: bool,
}

/// Resolve a type token, recursing through `list<...>` and `map<..,..>`.
///
/// Recursion is what makes the collection types worth having: `list<Match>`
/// and `map<string,double>` cost nothing extra once the element goes through
/// the same resolver as a bare field.
pub(crate) fn resolve_type(token: &str) -> Result<Resolved> {
    let token = token.trim();

    if let Some(inner) = generic_argument(token, "list") {
        let element = resolve_element(inner, token)?;
        let mut imports = element.imports;
        imports.push("java.util.List");
        return Ok(Resolved {
            java_type: format!("List<{}>", boxed(&element.java_type)),
            imports,
            owned: false,
            collection: true,
        });
    }

    if let Some(inner) = generic_argument(token, "map") {
        let (key, value) = inner.split_once(',').ok_or_else(|| {
            format!("'{token}' needs a key and a value type, e.g. map<string,double>")
        })?;
        let key = resolve_element(key, token)?;
        let value = resolve_element(value, token)?;
        let mut imports = key.imports;
        imports.extend(value.imports);
        imports.push("java.util.Map");
        return Ok(Resolved {
            java_type: format!(
                "Map<{}, {}>",
                boxed(&key.java_type),
                boxed(&value.java_type)
            ),
            imports,
            owned: false,
            collection: true,
        });
    }

    // The Java spellings of the built-ins, so `date:LocalDate` and `date:date`
    // mean the same thing and `id:String` is not read as a project type.
    if let Some((java_type, import)) = builtin_by_java_name(token) {
        return Ok(Resolved {
            java_type: java_type.to_string(),
            imports: import.into_iter().collect(),
            owned: false,
            collection: false,
        });
    }

    // Case is the whole rule: capitalised means a type this project owns.
    if token.starts_with(|c: char| c.is_uppercase()) {
        return Ok(Resolved {
            java_type: token.to_string(),
            imports: Vec::new(),
            owned: true,
            collection: false,
        });
    }

    let lower = token.to_lowercase();
    if lower == "list" || lower == "map" {
        return Err(format!(
            "'{token}' needs its element type(s) -- list<string>, list<Match>, map<string,double>"
        ));
    }

    let (java_type, import) = field_type(&lower)?;
    Ok(Resolved {
        java_type: java_type.to_string(),
        imports: import.into_iter().collect(),
        owned: false,
        collection: false,
    })
}

/// A collection's element type, with a message that names the collection it
/// came from -- `unknown field type 'nope'` alone is not much help when it
/// came out of `list<nope>`.
pub(crate) fn resolve_element(token: &str, outer: &str) -> Result<Resolved> {
    let token = token.trim();
    if token.is_empty() {
        return Err(format!("'{outer}' is missing an element type"));
    }
    let resolved = resolve_type(token).map_err(|e| format!("in '{outer}': {e}"))?;
    if resolved.collection {
        return Err(format!(
            "'{outer}': nested collections are not supported -- introduce a type for the inner one"
        ));
    }
    Ok(resolved)
}

/// The text inside `name<...>`, if the token is that shape. A bare `list` has
/// no element type and is meaningless, so it is not matched here and falls
/// through to the unknown-type error.
pub(crate) fn generic_argument<'a>(token: &'a str, name: &str) -> Option<&'a str> {
    token
        .strip_prefix(name)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

/// **The field-type vocabulary, and the only place it is written down.**
///
/// One row per accepted spelling: the token, the Java type it resolves to, and
/// the import that costs. Aliases are rows of their own (`string` and `text`,
/// `int` and `integer`), so the list a refusal prints is derived from what is
/// actually accepted rather than typed out beside it -- it used to be a literal
/// in the error message, which is a second list that goes stale the first time
/// somebody adds a type and does not scroll far enough.
///
/// `pending.md` §1.3 is the entry about that: *"the JSON sample table and the
/// field-type vocabulary are two spellings of one set. They were five apart,
/// which is how a `uri` component came to document a request its own record
/// refuses."* `every_builtin_type_has_a_json_sample` in `jails-generate` reads
/// [`builtin_java_types`] and fails when a sample table falls behind this one.
pub const BUILTIN_FIELD_TYPES: &[(&str, &str, Option<&str>)] = &[
    ("string", "String", None),
    ("text", "String", None),
    ("int", "Integer", None),
    ("integer", "Integer", None),
    ("long", "Long", None),
    ("boolean", "Boolean", None),
    ("date", "LocalDate", Some("java.time.LocalDate")),
    ("datetime", "LocalDateTime", Some("java.time.LocalDateTime")),
    ("instant", "Instant", Some("java.time.Instant")),
    ("uuid", "UUID", Some("java.util.UUID")),
    ("currency", "Currency", Some("java.util.Currency")),
    ("bigdecimal", "BigDecimal", Some("java.math.BigDecimal")),
    ("decimal", "BigDecimal", Some("java.math.BigDecimal")),
    ("bytes", "byte[]", None),
    ("duration", "Duration", Some("java.time.Duration")),
    ("zone-id", "ZoneId", Some("java.time.ZoneId")),
    ("zoneid", "ZoneId", Some("java.time.ZoneId")),
    ("uri", "URI", Some("java.net.URI")),
    ("path", "Path", Some("java.nio.file.Path")),
    ("double", "Double", None),
];

/// Every distinct Java type a built-in spelling can produce.
///
/// The set anything that maps a field to something else -- a JSON sample, a SQL
/// column, a fixture value -- has to cover. Order is the declaration order, so
/// a failure names the types in the order they appear above.
pub fn builtin_java_types() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (_, java, _) in BUILTIN_FIELD_TYPES {
        if !out.contains(java) {
            out.push(java);
        }
    }
    out
}

pub fn field_type(token: &str) -> Result<(&'static str, Option<&'static str>)> {
    if let Some((_, java, import)) = BUILTIN_FIELD_TYPES
        .iter()
        .find(|(spelling, _, _)| *spelling == token)
    {
        return Ok((java, *import));
    }
    let mut known: Vec<&str> = Vec::new();
    for (spelling, _, _) in BUILTIN_FIELD_TYPES {
        if !known.contains(spelling) {
            known.push(spelling);
        }
    }
    Err(format!(
        "unknown field type '{token}' (known: {}, list<T>, map<K,V>).\n       \
         Capitalise it -- {}:{} -- to mean a type this project owns.",
        known.join(", "),
        token,
        capitalize(token)
    ))
}

/// The Java spellings of the built-in table, so `date:LocalDate` and
/// `date:date` mean the same thing.
pub fn builtin_by_java_name(ty: &str) -> Option<(&'static str, Option<&'static str>)> {
    match ty {
        "String" => Some(("String", None)),
        "Integer" | "int" => Some(("Integer", None)),
        "Long" | "long" => Some(("Long", None)),
        "Boolean" | "boolean" => Some(("Boolean", None)),
        "Double" | "double" => Some(("Double", None)),
        "LocalDate" => Some(("LocalDate", Some("java.time.LocalDate"))),
        "LocalDateTime" => Some(("LocalDateTime", Some("java.time.LocalDateTime"))),
        "Instant" => Some(("Instant", Some("java.time.Instant"))),
        "UUID" => Some(("UUID", Some("java.util.UUID"))),
        "BigDecimal" => Some(("BigDecimal", Some("java.math.BigDecimal"))),
        "Duration" => Some(("Duration", Some("java.time.Duration"))),
        "ZoneId" => Some(("ZoneId", Some("java.time.ZoneId"))),
        "URI" => Some(("URI", Some("java.net.URI"))),
        "Path" => Some(("Path", Some("java.nio.file.Path"))),
        _ => None,
    }
}

pub fn parse_fields(args: &[String]) -> Result<Vec<Field>> {
    args.iter()
        .map(|arg| {
            let (name, ty) = arg
                .split_once(':')
                .ok_or_else(|| format!("field '{arg}' must be name:type"))?;

            let ty = ty.trim();
            // Table markers come off first, so `amount:long@pk!` and
            // `amount:long!@pk` mean the same thing rather than one of them
            // being a confusing parse error about an empty type.
            let (ty, constraints) = parse_constraints(ty, arg)?;
            let ty = ty.trim();
            let (ty, optionality) = match ty.strip_suffix('!') {
                Some(rest) => (rest, Optionality::NonBlank),
                None => match ty.strip_suffix('?') {
                    Some(rest) => (rest, Optionality::Nullable),
                    None => (ty, Optionality::Required),
                },
            };
            if ty.is_empty() {
                return Err(format!("field '{arg}' has a suffix but no type"));
            }
            derive_field(name.trim(), ty, optionality, constraints, arg)
        })
        .collect()
}

/// The Java facts a declared component implies, from parts already parsed.
///
/// **This is the half that is derivation rather than parsing**, and it is
/// separate so it can have a second caller.
/// `jails_protocol::declaration::FieldSpec` holds the same component in
/// validated form, one layer up; it used to reach a `Field` by rendering
/// itself back to a `name:type@marker` token and handing that to
/// [`parse_fields`] — a value this program had just parsed, printed, and
/// parsed again with the other of the two parsers. `FieldSpec::projected`
/// calls this instead.
///
/// `pending.md` §6.3: *"`java_type` and `imports` are derived facts computed by
/// a function on `FieldSpec`, not a second parse result."* This is that
/// function. The cross-checks stay here rather than moving up, because they are
/// about the *resolved Java type* — `!` needs a `String`, `@positive` needs a
/// numeric column — and only resolution knows what that is.
///
/// `arg` is the original token, used only so a refusal can quote what was
/// typed rather than a normalisation of it.
pub fn derive_field(
    name: &str,
    type_token: &str,
    optionality: Optionality,
    constraints: Constraints,
    arg: &str,
) -> Result<Field> {
    let resolved = resolve_type(type_token)?;
    if optionality == Optionality::NonBlank && resolved.java_type != "String" {
        return Err(format!(
            "'{arg}': the '!' suffix means non-blank, which only applies to text -- \
             drop it, or use '{name}:{type_token}' if you only meant required"
        ));
    }
    if optionality == Optionality::Nullable && resolved.collection {
        return Err(format!(
            "'{arg}': a collection already models absence as an empty one -- drop the '?'"
        ));
    }

    if let Some(check) = constraints.check {
        // A check jails cannot emit correctly is worse than none: the
        // migration would fail to apply, which is exactly the class of
        // failure the field spec is supposed to remove.
        if !is_numeric(&resolved.java_type) {
            return Err(format!(
                "'{arg}': {} only applies to a numeric column, and {} is not one",
                match check {
                    NumericCheck::Positive => "@positive",
                    NumericCheck::NonNegative => "@nonnegative",
                },
                resolved.java_type
            ));
        }
    }
    if constraints.primary_key && optionality == Optionality::Nullable {
        return Err(format!(
            "'{arg}': a primary key column cannot be nullable -- drop the '?' or the '@pk'"
        ));
    }

    Ok(Field {
        name: name.to_string(),
        java_type: resolved.java_type,
        imports: resolved.imports,
        optionality,
        owned: resolved.owned,
        collection: resolved.collection,
        constraints,
    })
}

/// Strip `@marker` suffixes off a field's type and read their constraints.
///
/// Repeatable and order-independent: `amount:long@positive@index` and
/// `amount:long@index@positive` are the same column. An unknown marker is an
/// error listing the real ones -- a typo that parsed as "no constraint" would
/// produce a schema quietly missing the primary key someone thought they had
/// asked for, which is the failure mode this whole feature exists to prevent.
pub(crate) fn parse_constraints<'a>(ty: &'a str, arg: &str) -> Result<(&'a str, Constraints)> {
    const KNOWN: &str = "@pk, @unique, @index, @scope, @positive, @nonnegative";
    let mut constraints = Constraints::default();
    let mut rest = ty;

    // Read markers right-to-left so the remaining head is still the type.
    while let Some(at) = rest.rfind('@') {
        let marker = rest[at + 1..].trim();
        match marker {
            "pk" => constraints.primary_key = true,
            "unique" => constraints.unique = true,
            "index" => constraints.indexed = true,
            "scope" => constraints.scoped = true,
            "positive" => constraints.check = Some(NumericCheck::Positive),
            "nonnegative" => constraints.check = Some(NumericCheck::NonNegative),
            "" => {
                return Err(format!(
                    "'{arg}': trailing '@' with no marker. Known: {KNOWN}"
                ));
            }
            other => {
                return Err(format!(
                    "'{arg}': unknown column marker '@{other}'. Known: {KNOWN}"
                ));
            }
        }
        rest = &rest[..at];
    }
    if rest.trim().is_empty() {
        return Err(format!("'{arg}': markers but no type"));
    }
    Ok((rest, constraints))
}

/// Java types a numeric `check` can be emitted against.
pub(crate) fn is_numeric(java_type: &str) -> bool {
    matches!(
        java_type,
        "long" | "Long" | "int" | "Integer" | "double" | "Double" | "BigDecimal"
    )
}

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Parse through boxed names so collection elements work, then use primitives
/// for required record/value components where null is not a meaningful state.
pub(crate) fn unboxed(java_type: &str) -> &str {
    match java_type {
        "Integer" => "int",
        "Long" => "long",
        "Boolean" => "boolean",
        "Double" => "double",
        other => other,
    }
}

/// A primitive component cannot be null, so it needs no runtime check.
pub(crate) fn is_reference_type(java_type: &str) -> bool {
    !matches!(java_type, "int" | "long" | "boolean" | "double")
}

/// A component gets a null check when it *can* be null and was not marked `?`.
pub fn needs_null_check(field: &Field) -> bool {
    !field.collection
        && is_reference_type(unboxed(&field.java_type))
        && field.optionality != Optionality::Nullable
}

/// Defensive copy plus an empty default, in one statement per collection.
///
/// Both halves matter and both are about the caller: a component holding the
/// list the caller passed in is not actually immutable, and a null bucket
/// makes every consumer downstream write a null check that should never have
/// been their problem.
pub fn collection_defaults(fields: &[Field]) -> String {
    fields
        .iter()
        .filter(|f| f.collection)
        .map(|f| {
            let empty = if f.java_type.starts_with("Map") {
                "Map.of()"
            } else {
                "List.of()"
            };
            let copy = if f.java_type.starts_with("Map") {
                "Map.copyOf"
            } else {
                "List.copyOf"
            };
            format!(
                "        {0} = {0} == null ? {empty} : {copy}({0});\n",
                f.name
            )
        })
        .collect()
}

pub fn has_collection(fields: &[Field]) -> bool {
    fields.iter().any(|f| f.collection)
}

/// The component's declared type. `?` wraps it in `Optional`, so absence is in
/// the type rather than in a comment nobody reads.
///
/// This is the one place jails deliberately parts company with `java.md`'s
/// "Optional as a return type only, never a field". A record component is both
/// at once, and the alternative -- a nullable component plus a differently
/// named `Optional`-returning method, since an accessor cannot be overridden
/// to change its return type -- is worse on every axis that matters here.
pub fn declared_type(field: &Field) -> String {
    match field.optionality {
        Optionality::Nullable => format!("Optional<{}>", boxed(&field.java_type)),
        _ if field.collection => field.java_type.clone(),
        _ => unboxed(&field.java_type).to_string(),
    }
}

/// `Optional<int>` does not exist, so an optional primitive takes its wrapper.
pub(crate) fn boxed(java_type: &str) -> &str {
    match java_type {
        "int" => "Integer",
        "long" => "Long",
        "boolean" => "Boolean",
        "double" => "Double",
        other => other,
    }
}

/// An `Optional` component still has to be non-null itself; a null `Optional`
/// is the one thing worse than a null value. Normalise rather than reject:
/// `of(..., null)` meaning "absent" is what every caller expects.
pub fn optional_defaults(fields: &[Field]) -> String {
    fields
        .iter()
        .filter(|f| f.optionality == Optionality::Nullable)
        .map(|f| {
            format!(
                "        {0} = Objects.requireNonNullElse({0}, Optional.empty());\n",
                f.name
            )
        })
        .collect()
}

pub fn has_optional(fields: &[Field]) -> bool {
    fields
        .iter()
        .any(|f| f.optionality == Optionality::Nullable)
}

/// Only `!` asks for the blank check, and only text can be blank.
pub fn needs_blank_check(field: &Field) -> bool {
    field.optionality == Optionality::NonBlank && field.java_type == "String"
}

/// Trim-then-reject, in that order, so " " fails rather than sneaking past.
pub fn blank_checks(fields: &[&Field]) -> String {
    let mut out = String::new();
    for field in fields {
        out += &format!("        {0} = {0}.trim();\n", field.name);
        out += &format!(
            "        if ({0}.isEmpty()) {{\n            throw new IllegalArgumentException(\"{0} must not be blank\");\n        }}\n",
            field.name
        );
    }
    out
}

// ---------------------------------------------------------------------------
// The same spec, read back off a record that already exists
// ---------------------------------------------------------------------------

/// The same question, asked of source the caller already has.
///
/// Split out so a projected project can answer it about a record that exists
/// only in the plan: an aggregate apply generates a scaffold and then a
/// search over it in one transition, and the second recipe has to see the
/// first one's record without either of them having been written.
pub fn fields_of_record(source: &str) -> Option<Vec<Field>> {
    let info = crate::java::type_info(source)?;
    if info.constructor_params.is_empty() {
        return None;
    }
    let fields: Vec<Field> = info
        .constructor_params
        .iter()
        .map(|param| {
            // An `Optional<T>` component is jails' `?` optionality; the rest
            // of the type resolves through the same table `parse_fields` uses,
            // so a hand-written record and a generated one map identically.
            let (java_type, optionality) = match param
                .raw_type
                .strip_prefix("Optional<")
                .and_then(|rest| rest.strip_suffix('>'))
            {
                Some(inner) => (inner.to_string(), Optionality::Nullable),
                None => (param.raw_type.clone(), Optionality::Required),
            };
            let builtin = builtin_by_java_name(&java_type);
            Field {
                name: param.name.clone(),
                // The *inner* type, exactly as `parse_fields` records it:
                // optionality lives in `optionality`, and `component_type`
                // is the one place that wraps it back into an `Optional`.
                // Two representations of the same thing is how a template
                // that works for one source of fields breaks for the other.
                java_type: java_type.clone(),
                imports: builtin.and_then(|(_, import)| import).into_iter().collect(),
                optionality,
                // A record read off disk carries no table markers: the Java
                // type cannot say what the column is. `g repo` on an existing
                // record therefore derives no constraints, which is honest --
                // guessing a primary key from a component called `id` is how
                // a schema ends up with one nobody asked for.
                constraints: Constraints::default(),
                owned: builtin.is_none(),
                collection: java_type.starts_with("List") || java_type.starts_with("Map"),
            }
        })
        .collect();
    Some(fields)
}
