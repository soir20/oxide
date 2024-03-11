mod deserialize;

use crate::deserialize::assign_fields;
use crate::deserialize::add_trait_bounds;
use syn::DeriveInput;
use quote::quote;
use syn::parse_macro_input;

#[proc_macro_derive(PacketDeserialize)]
pub fn derive_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let assignments = assign_fields(&input.data);

    let expanded = quote! {
        impl #impl_generics packet_serialize::PacketDeserialize for #name #ty_generics #where_clause {
            fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, PacketDeserializeError> {
                Ok(#name {
                    #assignments
                })
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
