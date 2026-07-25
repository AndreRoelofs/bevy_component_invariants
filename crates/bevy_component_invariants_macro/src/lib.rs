//! The attribute behind [`StateAxis`](../bevy_component_invariants/trait.StateAxis.html)
//! membership. See the `bevy_component_invariants` crate for the concept and the
//! docs; this crate only holds the expansion, because proc-macros must live alone,
//! and is not useful on its own.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Ident, ItemStruct, LitStr, Path, Token,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
};

/// Declares a component as one variant of an exclusive axis.
///
/// ```ignore
/// #[variant_of(ItemState, "on_ground")]
/// #[derive(Component, Clone, Copy)]
/// pub struct OnGround;
/// ```
///
/// The name is optional and defaults to the type's name in snake_case, so the
/// example above is the same as a bare `#[variant_of(ItemState)]`.
///
/// This must sit *above* `#[derive(Component)]`: the expansion injects the
/// `#[component(on_insert = ...)]` hook that does the excluding, and the derive
/// only sees attributes that are already present when it runs.
///
/// One attribute is deliberately the whole declaration. Being a variant and
/// enforcing exclusivity are the same fact, so there is no way to state one
/// without the other — `VariantOf` cannot be implemented by hand.
///
/// The expansion also submits the variant to the link-time collection
/// `AxisPlugin` drains, so it is in the registry before its first insert. Generic
/// variants are skipped there — there is no set of instantiations to collect, the
/// same limitation `#[derive(Reflect)]` has — and fall back to registering when
/// first inserted.
#[proc_macro_attribute]
pub fn variant_of(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as VariantArgs);
    let mut item = parse_macro_input!(item as ItemStruct);

    let name = args
        .name
        .map_or_else(|| snake_case(&item.ident), |name| name.value());
    let axis = &args.axis;
    let ty = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();

    let submit = auto_registration(&item);

    let expanded = quote! {
        #submit

        impl #impl_generics ::bevy_component_invariants::__private::Sealed
            for #ty #type_generics #where_clause {}

        impl #impl_generics ::bevy_component_invariants::VariantOf
            for #ty #type_generics #where_clause
        {
            type Axis = #axis;
            const KEY: ::bevy_component_invariants::VariantKey =
                ::bevy_component_invariants::VariantKey::new(
                    <#axis as ::bevy_component_invariants::StateAxis>::KEY,
                    #name,
                );
        }
    };

    // The derive collects `#[component(...)]` wherever it sits in the list, so
    // appending is enough — the attribute does not have to precede the derive.
    item.attrs.push(parse_quote! {
        #[component(on_insert = ::bevy_component_invariants::enforce_axis::<Self>)]
    });

    TokenStream::from(quote! {
        #item
        #expanded
    })
}

/// The submission that puts a variant in the registry before its first insert.
///
/// Empty for a generic variant: there is no set of instantiations to collect, so
/// it keeps the first-insert fallback and the hook says so once.
fn auto_registration(item: &ItemStruct) -> TokenStream2 {
    if !item.generics.params.is_empty() {
        return TokenStream2::new();
    }
    let ty = &item.ident;
    quote! {
        ::bevy_component_invariants::__private::inventory::submit! {
            ::bevy_component_invariants::VariantRegistration {
                key: <#ty as ::bevy_component_invariants::VariantOf>::KEY,
                register: ::bevy_component_invariants::register_component_of::<#ty>,
            }
        }
    }
}

struct VariantArgs {
    axis: Path,
    name: Option<LitStr>,
}

impl Parse for VariantArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let axis = input.parse::<Path>()?;
        let name = if input.parse::<Option<Token![,]>>()?.is_some() && !input.is_empty() {
            Some(input.parse::<LitStr>()?)
        } else {
            None
        };
        if !input.is_empty() {
            return Err(
                input.error("expected `#[variant_of(Axis)]` or `#[variant_of(Axis, \"name\")]`")
            );
        }
        Ok(Self { axis, name })
    }
}

/// `OnGround` -> `on_ground`, so the common case needs no string at all.
fn snake_case(ident: &Ident) -> String {
    let ident = ident.to_string();
    let mut out = String::with_capacity(ident.len() + 4);
    for (i, ch) in ident.char_indices() {
        if ch.is_uppercase() && i != 0 {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    out
}
