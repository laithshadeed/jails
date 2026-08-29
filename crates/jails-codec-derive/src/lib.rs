//! `#[derive(Codec)]` — the canonical wire framing, derived from the type.
//!
//! ## Why this exists
//!
//! `codec.rs` states the rule: one canonical encoding per type, and the
//! constructor is the only place a value is validated. What it could not state
//! is *where the field list lives*. A hand-written impl writes it three times —
//! in the struct, in `encode`, in `decode` — so a field added to the struct and
//! forgotten in the codec is a silent change of format rather than a compile
//! error, and 199 impls across the workspace say nothing except "these fields,
//! in this order".
//!
//! Deriving it makes the declaration the single owner of the encoding.
//!
//! ## What is normative
//!
//! - **A struct encodes its fields in declaration order.** Moving a field is
//!   therefore a format change, and reads as one in the diff.
//! - **An enum encodes a one-byte tag chosen explicitly**, never a Rust
//!   discriminant, because reordering variants must not renumber the wire.
//!   Every variant carries `#[codec(tag = N)]` and a missing one is a compile
//!   error rather than an inferred number.
//! - **A payload follows its tag** in field order, exactly as a struct does.
//! - **An unknown tag rejects.** There is no default variant: a value this
//!   binary cannot name is an error, not a silently substituted one.
//!
//! Every rule above is what the hand-written impls already did. The derive
//! reproduces their bytes exactly, which is what lets the golden ledgers stay
//! valid across the change.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, LitInt, parse_macro_input, spanned::Spanned};

#[proc_macro_derive(Codec, attributes(codec))]
pub fn derive_codec(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let (encode, decode) = match &input.data {
        Data::Struct(data) => struct_body(&data.fields)?,
        Data::Enum(data) => enum_body(&type_label(input)?, unknown_fix(input)?, data)?,
        Data::Union(_) => {
            return Err(syn::Error::new(
                input.span(),
                "a union has no canonical encoding; write the codec by hand",
            ));
        }
    };
    Ok(quote! {
        impl #impl_generics ::jails_support::codec::Codec for #name #type_generics #where_clause {
            fn encode(
                &self,
                encoder: &mut ::jails_support::codec::Encoder,
            ) -> ::jails_support::Result<()> {
                #encode
            }

            fn decode(
                decoder: &mut ::jails_support::codec::Decoder<'_>,
            ) -> ::jails_support::Result<Self> {
                #decode
            }
        }
    })
}

/// Fields in declaration order, for a struct or one variant's payload.
fn struct_body(
    fields: &Fields,
) -> syn::Result<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    match fields {
        Fields::Named(named) => {
            let names: Vec<_> = named
                .named
                .iter()
                .map(|f| f.ident.clone().unwrap())
                .collect();
            Ok((
                quote! {
                    #( ::jails_support::codec::Codec::encode(&self.#names, encoder)?; )*
                    Ok(())
                },
                quote! {
                    Ok(Self {
                        #( #names: ::jails_support::codec::Codec::decode(decoder)?, )*
                    })
                },
            ))
        }
        Fields::Unnamed(unnamed) => {
            let index: Vec<syn::Index> = (0..unnamed.unnamed.len()).map(syn::Index::from).collect();
            let slot: Vec<_> = (0..unnamed.unnamed.len())
                .map(|i| format_ident!("field{i}"))
                .collect();
            Ok((
                quote! {
                    #( ::jails_support::codec::Codec::encode(&self.#index, encoder)?; )*
                    Ok(())
                },
                quote! {
                    #( let #slot = ::jails_support::codec::Codec::decode(decoder)?; )*
                    Ok(Self( #(#slot),* ))
                },
            ))
        }
        Fields::Unit => Ok((quote! { Ok(()) }, quote! { Ok(Self) })),
    }
}

fn enum_body(
    label: &str,
    fix: Option<String>,
    data: &syn::DataEnum,
) -> syn::Result<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let mut encode_arms = Vec::new();
    let mut decode_arms = Vec::new();
    let mut seen: Vec<u8> = Vec::new();

    for variant in &data.variants {
        let ident = &variant.ident;
        let tag = variant_tag(variant)?;
        if seen.contains(&tag) {
            return Err(syn::Error::new(
                variant.span(),
                format!("tag {tag} is already used by another variant; wire tags must be unique"),
            ));
        }
        seen.push(tag);

        match &variant.fields {
            Fields::Unit => {
                encode_arms.push(quote! {
                    Self::#ident => { encoder.tag(#tag); Ok(()) }
                });
                decode_arms.push(quote! { #tag => Ok(Self::#ident) });
            }
            Fields::Named(named) => {
                let names: Vec<_> = named
                    .named
                    .iter()
                    .map(|f| f.ident.clone().unwrap())
                    .collect();
                encode_arms.push(quote! {
                    Self::#ident { #(#names),* } => {
                        encoder.tag(#tag);
                        #( ::jails_support::codec::Codec::encode(#names, encoder)?; )*
                        Ok(())
                    }
                });
                decode_arms.push(quote! {
                    #tag => Ok(Self::#ident {
                        #( #names: ::jails_support::codec::Codec::decode(decoder)?, )*
                    })
                });
            }
            Fields::Unnamed(unnamed) => {
                let slot: Vec<_> = (0..unnamed.unnamed.len())
                    .map(|i| format_ident!("field{i}"))
                    .collect();
                encode_arms.push(quote! {
                    Self::#ident( #(#slot),* ) => {
                        encoder.tag(#tag);
                        #( ::jails_support::codec::Codec::encode(#slot, encoder)?; )*
                        Ok(())
                    }
                });
                decode_arms.push(quote! {
                    #tag => {
                        #( let #slot = ::jails_support::codec::Codec::decode(decoder)?; )*
                        Ok(Self::#ident( #(#slot),* ))
                    }
                });
            }
        }
    }

    let refusal = match fix {
        None => quote! { format!("unknown {} tag {other}", #label) },
        Some(fix) => quote! {
            format!("unknown {} tag {other}.\n       fix: {}", #label, #fix)
        },
    };
    Ok((
        quote! { match self { #(#encode_arms),* } },
        quote! {
            match decoder.tag()? {
                #(#decode_arms,)*
                other => Err(::std::convert::Into::into(#refusal)),
            }
        },
    ))
}

/// What a refusal calls this type.
///
/// [`human_label`] is the default and is right most of the time, because a
/// hand-written codec spelled its refusal straight from the type name. Where a
/// reader knows the thing by another name -- `FileOp` is a "file operation",
/// `OneShotSpec` keeps its hyphen -- the type says so once, here. The
/// alternative is a message that drifts from the type it is about, which is
/// the drift this derive exists to remove.
fn type_label(input: &DeriveInput) -> syn::Result<String> {
    for attribute in &input.attrs {
        if !attribute.path().is_ident("codec") {
            continue;
        }
        let mut found = None;
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("label") {
                let literal: syn::LitStr = meta.value()?.parse()?;
                found = Some(literal.value());
                return Ok(());
            }
            if meta.path.is_ident("unknown_fix") {
                let _: syn::LitStr = meta.value()?.parse()?;
                return Ok(());
            }
            Err(meta.error("expected `label = \"...\"` or `unknown_fix = \"...\"`"))
        })?;
        if let Some(label) = found {
            return Ok(label);
        }
    }
    Ok(human_label(&input.ident.to_string()))
}

/// What to tell a reader who hit a tag this binary cannot name.
///
/// Eleven hand-written codecs ended their refusal with a `fix:` line, because
/// an unknown tag is not a bug report -- it is a reader holding state a
/// different jails wrote, and the repair is the same every time. Dropping it
/// when the codec became a derive would have quietly made those messages
/// worse, so the type carries it.
fn unknown_fix(input: &DeriveInput) -> syn::Result<Option<String>> {
    for attribute in &input.attrs {
        if !attribute.path().is_ident("codec") {
            continue;
        }
        let mut found = None;
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("unknown_fix") {
                let literal: syn::LitStr = meta.value()?.parse()?;
                found = Some(literal.value());
                return Ok(());
            }
            if meta.path.is_ident("label") {
                let _: syn::LitStr = meta.value()?.parse()?;
                return Ok(());
            }
            Err(meta.error("expected `label = \"...\"` or `unknown_fix = \"...\"`"))
        })?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// The explicit wire number. There is deliberately no default: an inferred tag
/// would renumber the wire the first time somebody reordered the variants.
fn variant_tag(variant: &syn::Variant) -> syn::Result<u8> {
    for attribute in &variant.attrs {
        if !attribute.path().is_ident("codec") {
            continue;
        }
        let mut found = None;
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("tag") {
                let literal: LitInt = meta.value()?.parse()?;
                found = Some(literal.base10_parse::<u8>()?);
                return Ok(());
            }
            Err(meta.error("expected `tag = N`"))
        })?;
        if let Some(tag) = found {
            return Ok(tag);
        }
    }
    Err(syn::Error::new(
        variant.span(),
        "every wire variant needs an explicit `#[codec(tag = N)]`; \
         a derived discriminant would renumber the wire when variants are reordered",
    ))
}

/// `ScalarFieldType` -> `"scalar field type"`.
///
/// Every hand-written codec in the workspace spelled its refusal that way —
/// "unknown semantic edit tag 9", not "unknown SemanticEdit tag 9" — because a
/// refusal is read by a person, not by the compiler. Deriving the label from
/// the type name reproduces that convention exactly, so replacing a hand codec
/// with the derive does not quietly change what a reader is told.
fn human_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in name.char_indices() {
        if character.is_ascii_uppercase() && index != 0 {
            out.push(' ');
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::human_label;

    #[test]
    fn the_label_matches_the_convention_the_hand_written_codecs_used() {
        assert_eq!(human_label("FieldType"), "field type");
        assert_eq!(human_label("ScalarFieldType"), "scalar field type");
        assert_eq!(human_label("SemanticEdit"), "semantic edit");
        assert_eq!(human_label("OwnerId"), "owner id");
    }
}
