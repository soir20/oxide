use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_quote, DataStruct, Fields, GenericParam, Generics, Ident, Index};

pub fn add_struct_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(packet_serialize::SerializePacket));
        }
    }
    generics
}

pub fn add_enum_trait_bounds(mut generics: Generics, repr: &Ident) -> Generics {
    generics = add_struct_trait_bounds(generics);
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(std::convert::Into<#repr>));
        }
    }
    generics
}

pub fn write_struct_fields(data: &DataStruct) -> TokenStream {
    match data.fields {
        Fields::Named(ref fields) => {
            let writes = fields.named.iter().map(|f| {
                let name = &f.ident;
                quote_spanned! {f.span()=>
                    packet_serialize::SerializePacket::serialize(&self.#name, buffer);
                }
            });
            quote! {
                #(
                    #writes
                )*
            }
        }
        Fields::Unnamed(ref fields) => {
            let writes = fields.unnamed.iter().enumerate().map(|(i, f)| {
                let index = Index::from(i);
                quote_spanned! {f.span()=>
                    packet_serialize::SerializePacket::serialize(&self.#index, buffer);
                }
            });
            quote! {
                #(
                    #writes
                )*
            }
        }
        Fields::Unit => {
            quote!()
        }
    }
}

pub fn write_enum(repr: &Ident) -> TokenStream {
    quote! {
        let primitive: #repr = std::convert::Into::into(*self);
        packet_serialize::SerializePacket::serialize(&primitive, buffer);
    }
}
