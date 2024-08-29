use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse2, parse_quote, Data, DeriveInput, Error, GenericParam, LitInt, Result, Type};

use crate::util::{make_tuple, replace_lifetime};

pub(crate) fn derive_query(input: TokenStream) -> Result<TokenStream> {
    let mut input = parse2::<DeriveInput>(input)?;
    let name = &input.ident;

    let lifetimes;
    let tuple_ty;
    let get_body;

    let mut ro_generics = input.generics.clone();

    match input.data {
        Data::Struct(struct_) => {
            lifetimes = input
                .generics
                .params
                .iter()
                .filter_map(|p| match p {
                    GenericParam::Lifetime(life) => Some(life.clone()),
                    GenericParam::Type(_) => None,
                    GenericParam::Const(_) => None,
                })
                .collect::<Vec<_>>();

            tuple_ty = make_tuple(struct_.fields.iter().map(|f| &f.ty));

            let where_clause = input.generics.make_where_clause();

            let ro_where_clause = ro_generics.make_where_clause();

            for field in &struct_.fields {
                let ty = &field.ty;

                let mut replaced_ty = ty.clone();

                for life in &lifetimes {
                    replace_lifetime(&mut replaced_ty, &life.lifetime.ident, &parse_quote!(__a));
                }

                where_clause.predicates.push(
                    parse_quote!(#ty: for<'__a> ::evenio::query::Query<This<'__a> = #replaced_ty>),
                );

                ro_where_clause
                    .predicates
                    .push(parse_quote!(#ty: for<'__a> ::evenio::query::ReadOnlyQuery<This<'__a> = #replaced_ty>));
            }

            get_body = match &struct_.fields {
                syn::Fields::Named(fields) => {
                    let idents: Vec<_> = fields
                        .named
                        .iter()
                        .map(|f| f.ident.clone().unwrap())
                        .collect();

                    let underscored_idents: Vec<_> = idents
                        .iter()
                        .map(|i| Ident::new(&format!("__{i}"), Span::call_site()))
                        .collect();

                    quote! {
                        let (#(#underscored_idents,)*) = <#tuple_ty as ::evenio::query::Query>::get(state, row);

                        #name {
                            #(#idents: #underscored_idents),*
                        }
                    }
                }
                syn::Fields::Unnamed(fields) => {
                    let indices = fields
                        .unnamed
                        .iter()
                        .enumerate()
                        .map(|(i, _)| LitInt::new(&format!("{i}"), Span::call_site()));

                    quote! {
                        let __tuple = <#tuple_ty as ::evenio::query::Query>::get(state, row);

                        #name(#(__tuple.#indices),*)
                    }
                }
                syn::Fields::Unit => quote!(#name),
            };
        }
        Data::Enum(_) => {
            return Err(Error::new(
                Span::call_site(),
                "cannot derive `Query` on enums",
            ))
        }
        Data::Union(_) => {
            return Err(Error::new(
                Span::call_site(),
                "cannot derive `Query` on unions",
            ))
        }
    }

    let (ro_impl_generics, ro_ty_generics, ro_where_clause) = ro_generics.split_for_impl();

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut this: Type = parse_quote!(#name #ty_generics);
    for life in &lifetimes {
        replace_lifetime(&mut this, &life.lifetime.ident, &parse_quote!(__a));
    }

    Ok(quote! {
        #[automatically_derived]
        unsafe impl #impl_generics ::evenio::query::Query for #name #ty_generics #where_clause {
            type This<'__a> = #this;

            type ArchState = <#tuple_ty as ::evenio::query::Query>::ArchState;

            type State = <#tuple_ty as ::evenio::query::Query>::State;

            fn init(
                state: &Self::State,
                config: &mut ::evenio::handler::HandlerConfig
            ) -> ::core::result::Result<::evenio::access::ComponentAccess, ::evenio::handler::InitError>
            {
                <#tuple_ty as ::evenio::query::Query>::init(state, config)
            }

            fn new_state(world: &mut ::evenio::world::World) -> Self::State {
                <#tuple_ty as ::evenio::query::Query>::new_state(world)
            }

            fn get_new_state(world: &::evenio::world::World) -> Option<Self::State> {
                <#tuple_ty as ::evenio::query::Query>::get_new_state(world)
            }

            fn new_arch_state(arch: &::evenio::archetype::Archetype, state: &mut Self::State) -> Option<Self::ArchState> {
                <#tuple_ty as ::evenio::query::Query>::new_arch_state(arch, state)
            }

            unsafe fn get<'__a>(state: &Self::ArchState, row: ::evenio::archetype::ArchetypeRow) -> Self::This<'__a> {
                #get_body
            }
        }

        #[automatically_derived]
        unsafe impl #ro_impl_generics ::evenio::query::ReadOnlyQuery for #name #ro_ty_generics #ro_where_clause {}
    })
}
