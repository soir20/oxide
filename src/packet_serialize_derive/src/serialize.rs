use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, Fields, GenericParam, Generics, Index, parse_quote};

pub fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(packet_serialize::SerializePacket));
        }
    }
    generics
}

pub fn write_fields(data: &Data) -> TokenStream {
    match *data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let writes = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    quote_spanned! {f.span()=>
                        packet_serialize::SerializePacket::serialize(&self.#name, buffer)?;
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
                        packet_serialize::SerializePacket::serialize(&self.#index, buffer)?;
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
        },
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}
