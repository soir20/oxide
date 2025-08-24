mod deserialize;
mod serialize;

use quote::quote;
use syn::parse_macro_input;
use syn::DeriveInput;

use crate::deserialize::add_enum_trait_bounds;
use crate::deserialize::add_struct_trait_bounds;
use crate::deserialize::assign_enum_variant;
use crate::deserialize::assign_struct_fields;

#[proc_macro_derive(SerializePacket)]
pub fn derive_serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let generics = serialize::add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let writes = serialize::write_fields(&input.data);

    let expanded = quote! {
        impl #impl_generics packet_serialize::SerializePacket for #name #ty_generics #where_clause {
            fn serialize(&self, buffer: &mut Vec<u8>) {
                #writes
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(DeserializePacket)]
pub fn derive_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let (generics, body) = match &input.data {
        syn::Data::Struct(data) => {
            let assignments = assign_struct_fields(data);
            (
                add_struct_trait_bounds(input.generics),
                quote! {
                    Ok(#name {
                        #assignments
                    })
                },
            )
        }
        syn::Data::Enum(_) => (add_enum_trait_bounds(input.generics), assign_enum_variant()),
        syn::Data::Union(_) => unimplemented!(),
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics packet_serialize::DeserializePacket for #name #ty_generics #where_clause {
            fn deserialize(cursor: &mut std::io::Cursor<&[u8]>) -> Result<Self, packet_serialize::DeserializePacketError> {
                #body
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
