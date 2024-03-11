mod deserialize;
mod serialize;

use syn::DeriveInput;
use quote::quote;
use syn::parse_macro_input;

#[proc_macro_derive(PacketSerialize)]
pub fn derive_serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let generics = serialize::add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let writes = serialize::write_fields(&input.data);

    let expanded = quote! {
        impl #impl_generics packet_serialize::PacketSerialize for #name #ty_generics #where_clause {
            fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), packet_serialize::PacketSerializeError> {
                #writes
                Ok(())
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(PacketDeserialize)]
pub fn derive_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let generics = deserialize::add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let assignments = deserialize::assign_fields(&input.data);

    let expanded = quote! {
        impl #impl_generics packet_serialize::PacketDeserialize for #name #ty_generics #where_clause {
            fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, packet_serialize::PacketDeserializeError> {
                Ok(#name {
                    #assignments
                })
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
