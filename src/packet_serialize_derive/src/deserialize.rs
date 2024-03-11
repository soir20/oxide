use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{Data, Fields, GenericParam, Generics, Index, parse_quote};
use syn::spanned::Spanned;

pub fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(parse_quote!(packet_serialize::PacketDeserialize));
        }
    }
    generics
}

pub fn assign_fields(data: &Data) -> TokenStream {
    match *data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let assignments = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        quote_spanned! {f.span()=>
                            #name: packet_serialize::PacketDeserialize::deserialize(cursor)?,
                        }
                    });
                    quote! {
                        #(
                            #assignments
                        )*
                    }
                }
                Fields::Unnamed(ref fields) => {
                    let assignments = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        quote_spanned! {f.span()=>
                            #index: packet_serialize::PacketDeserialize::deserialize(cursor)?,
                        }
                    });
                    quote! {
                        #(
                            #assignments
                        )*
                    }
                }
                Fields::Unit => {
                    quote!()
                }
            }
        }
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}
