mod deserialize;
mod serialize;

use quote::quote;
use syn::parse::Parse;
use syn::parse_macro_input;
use syn::parse_quote;
use syn::DeriveInput;
use syn::Ident;
use syn::Meta;

struct ExtendedDeriveInput {
    base: DeriveInput,
    repr: Option<Ident>,
}

impl ExtendedDeriveInput {
    fn parse_repr(input: &DeriveInput) -> syn::Result<Option<Ident>> {
        for attr in &input.attrs {
            let Meta::List(list) = &attr.meta else {
                continue;
            };

            let Some(ident) = list.path.get_ident() else {
                continue;
            };

            if ident != "repr" {
                continue;
            }

            let mut tokens = list.tokens.clone().into_iter();
            let token_tree = match (tokens.next(), tokens.next()) {
                (Some(token_tree), None) => token_tree,
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "Expected exactly one `repr` argument",
                    ))
                }
            };

            return Ok(Some(parse_quote! { #token_tree }));
        }

        return Ok(None);
    }
}

impl Parse for ExtendedDeriveInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let base: DeriveInput = input.parse()?;
        let repr = Self::parse_repr(&base)?;

        Ok(ExtendedDeriveInput { base, repr })
    }
}

#[proc_macro_derive(SerializePacket)]
pub fn derive_serialize(token_stream: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(token_stream as ExtendedDeriveInput);

    let name = input.base.ident;

    let body = match &input.base.data {
        syn::Data::Struct(data) => serialize::write_struct_fields(data),
        syn::Data::Enum(_) => {
            let Some(repr) = input.repr else {
                let err = syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Missing `repr` argument for enum",
                );
                return proc_macro::TokenStream::from(err.to_compile_error());
            };
            serialize::write_enum(&repr)
        }
        syn::Data::Union(_) => unimplemented!(),
    };

    let (impl_generics, ty_generics, where_clause) = input.base.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics packet_serialize::SerializePacket for #name #ty_generics #where_clause {
            fn serialize(&self, buffer: &mut Vec<u8>) {
                #body
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(DeserializePacket)]
pub fn derive_deserialize(token_stream: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(token_stream as DeriveInput);

    let name = input.ident;

    let body = match &input.data {
        syn::Data::Struct(data) => {
            let assignments = deserialize::assign_struct_fields(data);
            quote! {
                Ok(#name {
                    #assignments
                })
            }
        }
        syn::Data::Enum(_) => deserialize::assign_enum_variant(),
        syn::Data::Union(_) => unimplemented!(),
    };

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics packet_serialize::DeserializePacket for #name #ty_generics #where_clause {
            fn deserialize(cursor: &mut std::io::Cursor<&[u8]>) -> Result<Self, packet_serialize::DeserializePacketError> {
                #body
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
