//! A component the caller pins to a constant, resolved against its declared
//! type.
//!
//! `POST /admin_api/messages` must write `sender_type = ADMIN` and
//! `POST /customer_api/messages` must write `CUSTOMER`. With the component in
//! the request both endpoints take it from the caller, so either one can forge
//! the other's rows -- and no validation on the request closes that, because a
//! well-formed request is exactly what the forgery looks like. The value has
//! to come from the *endpoint*, which means from generation time.
//!
//! **The literal is resolved here, not passed through.** `LiteralValue` has
//! already refused anything an expression could hide in; this module answers
//! the second question, which is whether the literal means anything for the
//! type the component was declared as. `--set senderType=SHOUTING` on an enum
//! with two constants is a refusal naming both, not `SenderType.SHOUTING`
//! written into a constructor argument for `javac` to find.

use super::*;
use jails_support::Result;

/// One resolved pin: the component, the Java expression it becomes, and what
/// that expression has to import.
pub(crate) struct Pin {
    pub(crate) component: String,
    pub(crate) expression: String,
    pub(crate) imports: Vec<String>,
}

/// What a pin is being resolved *for*, so a refusal can name the command that
/// produced it.
#[derive(Clone, Copy)]
pub(crate) struct Pinning<'a> {
    /// The recipe name as the CLI spells it -- `usecase`, `transition`.
    pub(crate) recipe: &'a str,
    /// The generated class's name.
    pub(crate) name: &'a str,
    /// The resource whose components are being pinned.
    pub(crate) target: &'a str,
}

/// Resolve every `component=literal` token against the target's declared
/// components.
///
/// `request` is what the caller still sends, and a component in both is the
/// one refusal that matters most: a pin the request can override is not a pin
/// at all, and the endpoint would be back to trusting the caller for the value
/// it exists to decide.
pub(crate) fn resolve(
    slice: &Slice,
    pinning: Pinning<'_>,
    fields: (&[crate::generate::Field], &[crate::generate::Field]),
    tokens: &[String],
) -> Result<Vec<Pin>> {
    let (target_fields, request) = fields;
    let Pinning {
        recipe,
        name,
        target,
    } = pinning;
    let mut pins: Vec<Pin> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let spec = jails_protocol::declaration::PinSpec::parse(token)?;
        let component = spec.component.to_string();
        if let Some(earlier) = pins.iter().find(|pin| pin.component == component) {
            return Err(format!(
                "{recipe} {name} pins `{component}` twice.\n       fix: one `--set` per \
                 component; it already holds `{}`.",
                earlier.expression
            )
            .into());
        }
        let Some(field) = target_fields
            .iter()
            .find(|candidate| candidate.name == component)
        else {
            return Err(format!(
                "{recipe} {name} pins `{component}`, but {target} has no component with that \
                 name.\n       fix: {target} declares {}.",
                declared(target_fields)
            )
            .into());
        };
        if request.iter().any(|input| input.name == component) {
            return Err(format!(
                "{recipe} {name} both accepts `{component}` and pins it to `{}`.\n       fix: \
                 drop one. A pinned component the request can override is not pinned -- the \
                 endpoint would still be taking the value from the caller, which is what \
                 pinning it exists to stop.",
                spec.value
            )
            .into());
        }
        let (expression, imports) = expression(slice, recipe, name, field, spec.value.as_str())?;
        pins.push(Pin {
            component,
            expression,
            imports,
        });
    }
    Ok(pins)
}

/// The Java a literal becomes for one declared component.
fn expression(
    slice: &Slice,
    recipe: &str,
    name: &str,
    field: &crate::generate::Field,
    literal: &str,
) -> Result<(String, Vec<String>)> {
    let component = &field.name;
    if field.collection {
        return Err(format!(
            "{recipe} {name} pins `{component}`, which is a {}.\n       fix: a pinned value is \
             one literal, and a collection is not one. Leave it out and let the create supply \
             the empty collection it already does.",
            field.java_type
        )
        .into());
    }
    let (raw, imports) = scalar(slice, recipe, name, field, literal)?;
    // An `Optional<T>` component takes the value wrapped, the same shape the
    // record's own compact constructor normalises to. Pinning it to a present
    // value is meaningful; there is no spelling here for pinning it to empty,
    // because leaving it out already does that.
    if field.optionality == crate::generate::Optionality::Nullable {
        let mut imports = imports;
        imports.push("java.util.Optional".to_string());
        return Ok((format!("Optional.of({raw})"), imports));
    }
    Ok((raw, imports))
}

fn scalar(
    slice: &Slice,
    recipe: &str,
    name: &str,
    field: &crate::generate::Field,
    literal: &str,
) -> Result<(String, Vec<String>)> {
    let component = &field.name;
    // Every arm that succeeds returns; every arm that does not yields the one
    // thing it wanted instead, and the refusal is worded once below. Two arms
    // built the message through a helper before, which read the same and was
    // not: a refusal assembled somewhere else is one nothing can check still
    // says what to do next.
    let expected: &str = match field.java_type.as_str() {
        "boolean" | "Boolean" => match literal {
            "true" | "false" => return Ok((literal.to_string(), Vec::new())),
            _ => "write `true` or `false`.",
        },
        "int" | "Integer" => match literal.parse::<i32>() {
            Ok(value) => return Ok((value.to_string(), Vec::new())),
            Err(_) => "write a whole number that fits in an `int`.",
        },
        "long" | "Long" => match literal.parse::<i64>() {
            Ok(value) => return Ok((format!("{value}L"), Vec::new())),
            Err(_) => "write a whole number that fits in a `long`.",
        },
        "short" | "Short" => match literal.parse::<i16>() {
            Ok(value) => return Ok((format!("(short) {value}"), Vec::new())),
            Err(_) => "write a whole number that fits in a `short`.",
        },
        "byte" | "Byte" => match literal.parse::<i8>() {
            Ok(value) => return Ok((format!("(byte) {value}"), Vec::new())),
            Err(_) => "write a whole number that fits in a `byte`.",
        },
        "double" | "Double" => match literal.parse::<f64>() {
            Ok(value) => return Ok((format!("{value:?}d"), Vec::new())),
            Err(_) => "write a number.",
        },
        "float" | "Float" => match literal.parse::<f32>() {
            Ok(value) => return Ok((format!("{value:?}f"), Vec::new())),
            Err(_) => "write a number.",
        },
        // The alphabet `LiteralValue` enforces contains no quote, backslash or
        // newline, so this is the whole of the escaping and there is no case
        // where it is not.
        "String" => return Ok((format!("\"{literal}\""), Vec::new())),
        // A project type. The one jails can resolve a constant of is an enum,
        // which it reads off disk -- the same read `g usecase`'s status
        // default already does, and the reason `g enum` earns its place twice.
        owned if field.owned => {
            let domain: &str = &slice.owned(Layer::Domain);
            let Some(constants) = crate::generate::enum_constants(slice.project(), domain, owned)
            else {
                return Err(format!(
                    "{recipe} {name} pins `{component}` to `{literal}`, and {owned} is not an \
                     enum this project declares.\n       fix: a pinned value is a literal, so \
                     the component has to be a builtin or an enum jails can read. \
                     `jails g enum {owned} ...` writes one."
                )
                .into());
            };
            if !constants.iter().any(|constant| constant == literal) {
                return Err(format!(
                    "{recipe} {name} pins `{component}` to `{literal}`, which is not a constant \
                     of {owned}.\n       fix: {owned} declares {}.",
                    constants.join(", ")
                )
                .into());
            }
            return Ok((
                format!("{owned}.{literal}"),
                vec![format!("{domain}.{owned}")],
            ));
        }
        // Deliberately closed. A pinned `Instant` is a timestamp frozen at
        // generation time, which is never what anyone means, and a pinned
        // `UUID` is a constant primary key. Both compile and neither is right,
        // so the refusal is the useful answer.
        _ => {
            "a pinned value is a boolean, a number, a piece of text, or a constant of an enum \
             this project declares -- not a value with a lifetime of its own."
        }
    };
    Err(format!(
        "{recipe} {name} pins `{component}` to `{literal}`, and {component} is declared \
         `{}`.\n       fix: {expected}",
        field.java_type
    )
    .into())
}

/// The target's components, for a refusal that says what could have been
/// pinned instead of only what could not.
fn declared(fields: &[crate::generate::Field]) -> String {
    if fields.is_empty() {
        return "no components".to_string();
    }
    fields
        .iter()
        .map(|field| format!("`{}`", field.name))
        .collect::<Vec<_>>()
        .join(", ")
}
