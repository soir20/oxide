use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{DataStruct, Fields, Index};

pub fn assign_struct_fields(data: &DataStruct) -> TokenStream {
    match data.fields {
        Fields::Named(ref fields) => {
            let assignments = fields.named.iter().map(|f| {
                let name = &f.ident;
                quote_spanned! {f.span()=>
                    #name: packet_serialize::DeserializePacket::deserialize(cursor)?,
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
                    #index: packet_serialize::DeserializePacket::deserialize(cursor)?,
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

pub fn assign_enum_variant() -> TokenStream {
    quote! {
        let primitive = packet_serialize::DeserializePacket::deserialize(cursor)?;
        Self::try_from_primitive(primitive).map_err(|_| packet_serialize::DeserializePacketError::UnknownDiscriminator)
    }
}
