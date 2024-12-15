extern crate proc_macro;
use alloy_core::primitives::keccak256;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, FnArg, ImplItem, ImplItemMethod, ItemImpl,
    ItemTrait, ReturnType, TraitItem,
};

mod helpers;

#[proc_macro_derive(Event, attributes(indexed))]
pub fn event_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            &fields.named
        } else {
            panic!("Event must have named fields");
        }
    } else {
        panic!("Event must be a struct");
    };

    // Collect iterators into vectors
    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    let indexed_fields: Vec<_> = fields
        .iter()
        .filter(|f| f.attrs.iter().any(|attr| attr.path.is_ident("indexed")))
        .map(|f| &f.ident)
        .collect();

    let expanded = quote! {
        impl #name {
            const NAME: &'static str = stringify!(#name);
            const INDEXED_FIELDS: &'static [&'static str] = &[
                #(stringify!(#indexed_fields)),*
            ];

            pub fn new(#(#field_names: #field_types),*) -> Self {
                Self {
                    #(#field_names),*
                }
            }
        }

        impl eth_riscv_runtime::log::Event for #name {
            fn encode_log(&self) -> (alloc::vec::Vec<u8>, alloc::vec::Vec<[u8; 32]>) {
                use alloy_sol_types::SolValue;
                use alloy_core::primitives::{keccak256, B256};
                use alloc::vec::Vec;

                let mut signature = Vec::new();
                signature.extend_from_slice(Self::NAME.as_bytes());
                signature.extend_from_slice(b"(");

                let mut first = true;
                let mut topics = alloc::vec![B256::default()];
                let mut data = Vec::new();

                #(
                    if !first { signature.extend_from_slice(b","); }
                    first = false;

                    signature.extend_from_slice(self.#field_names.sol_type_name().as_bytes());
                    let encoded = self.#field_names.abi_encode();

                    let field_name = stringify!(#field_names);
                    if Self::INDEXED_FIELDS.contains(&field_name) && topics.len() < 4 {
                        topics.push(B256::from_slice(&encoded));
                    } else {
                        data.extend_from_slice(&encoded);
                    }
                )*

                signature.extend_from_slice(b")");
                topics[0] = B256::from(keccak256(&signature));

                (data, topics.iter().map(|t| t.0).collect())
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn show_streams(attr: TokenStream, item: TokenStream) -> TokenStream {
    println!("attr: \"{}\"", attr.to_string());
    println!("item: \"{}\"", item.to_string());
    item
}

#[proc_macro_attribute]
pub fn contract(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let struct_name = if let syn::Type::Path(type_path) = &*input.self_ty {
        &type_path.path.segments.first().unwrap().ident
    } else {
        panic!("Expected a struct.");
    };

    let mut public_methods: Vec<&ImplItemMethod> = Vec::new();

    // Iterate over the items in the impl block to find pub methods
    for item in input.items.iter() {
        if let ImplItem::Method(method) = item {
            if let syn::Visibility::Public(_) = method.vis {
                public_methods.push(method);
            }
        }
    }

    let match_arms: Vec<_> = public_methods.iter().enumerate().map(|(_, method)| {
        let method_name = &method.sig.ident;
        let method_selector = u32::from_be_bytes(
            keccak256(
                method_name.to_string()
            )[..4].try_into().unwrap_or_default()
        );
        let arg_types: Vec<_> = method.sig.inputs.iter().skip(1).map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                let ty = &*pat_type.ty;
                quote! { #ty }
            } else {
                panic!("Expected typed arguments");
            }
        }).collect();

        let arg_names: Vec<_> = (0..method.sig.inputs.len() - 1).map(|i| format_ident!("arg{}", i)).collect();
        let checks = if !is_payable(&method) {
            quote! {
                if eth_riscv_runtime::msg_value() > U256::from(0) {
                    revert();
                }
            }
        } else {
            quote! {}
        };
        // Check if the method has a return type
        let return_handling = match &method.sig.output {
            ReturnType::Default => {
                // No return value
                quote! {
                    self.#method_name(#( #arg_names ),*);
                }
            }
            ReturnType::Type(_, return_type) => {
                // Has return value
                quote! {
                    let result: #return_type = self.#method_name(#( #arg_names ),*);
                    let result_bytes = result.abi_encode();
                    let result_size = result_bytes.len() as u64;
                    let result_ptr = result_bytes.as_ptr() as u64;
                    return_riscv(result_ptr, result_size);
                }
            }
        };

        quote! {
            #method_selector => {
                let (#( #arg_names ),*) = <(#( #arg_types ),*)>::abi_decode(calldata, true).unwrap();
                #checks
                #return_handling
            }
        }
    }).collect();

    let emit_helper = quote! {
        #[macro_export]
        macro_rules! get_type_signature {
            ($arg:expr) => {
                $arg.sol_type_name().as_bytes()
            };
        }

        #[macro_export]
        macro_rules! emit {
            ($event:ident, $($field:expr),*) => {{
                use alloy_sol_types::SolValue;
                use alloy_core::primitives::{keccak256, B256, U256, I256};
                use alloc::vec::Vec;

                let mut signature = alloc::vec![];
                signature.extend_from_slice($event::NAME.as_bytes());
                signature.extend_from_slice(b"(");

                let mut first = true;
                let mut topics = alloc::vec![B256::default()];
                let mut data = Vec::new();

                $(
                    if !first { signature.extend_from_slice(b","); }
                    first = false;

                    signature.extend_from_slice(get_type_signature!($field));
                    let encoded = $field.abi_encode();

                    let field_ident = stringify!($field);
                    if $event::INDEXED_FIELDS.contains(&field_ident) && topics.len() < 4 {
                        topics.push(B256::from_slice(&encoded));
                    } else {
                        data.extend_from_slice(&encoded);
                    }
                )*

                signature.extend_from_slice(b")");
                topics[0] = B256::from(keccak256(&signature));

                if !data.is_empty() {
                    eth_riscv_runtime::emit_log(&data, &topics);
                } else if topics.len() > 1 {
                    let data = topics.pop().unwrap();
                    eth_riscv_runtime::emit_log(data.as_ref(), &topics);
                }
            }};
        }
    };

    // Generate the interface
    let interface_name = format_ident!("I{}", struct_name);
    let interface = helpers::generate_interface(&public_methods, &interface_name);

    // Generate the complete output with module structure
    let output = quote! {
        // Public interface module
        pub mod interface {
            use super::*;
            #interface
        }

        // Generate the call method implementation privately
        // only when not in `interface-only` mode
        #[cfg(not(feature = "interface-only"))]
        mod implementation {
            use super::*;
            use alloy_sol_types::SolValue;
            use eth_riscv_runtime::*;

            #input

            #emit_helper

            impl Contract for #struct_name {
                fn call(&self) {
                    self.call_with_data(&msg_data());
                }

                fn call_with_data(&self, calldata: &[u8]) {
                    let selector = u32::from_be_bytes([calldata[0], calldata[1], calldata[2], calldata[3]]);
                    let calldata = &calldata[4..];

                    match selector {
                        #( #match_arms )*
                        _ => revert(),
                    }

                    return_riscv(0, 0);
                }
            }

            #[eth_riscv_runtime::entry]
            fn main() -> ! {
                let contract = #struct_name::default();
                contract.call();
                eth_riscv_runtime::return_riscv(0, 0)
            }
        }

        // Always export the interface
        pub use interface::*;

        // Only export contract impl when not in `interface-only` mode
        #[cfg(not(feature = "interface-only"))]
        pub use implementation::*;
    };

    TokenStream::from(output)
}

// Empty macro to mark a method as payable
#[proc_macro_attribute]
pub fn payable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

// Check if a method is tagged with the payable attribute
fn is_payable(method: &syn::ImplItemMethod) -> bool {
    method.attrs.iter().any(|attr| {
        if let Ok(syn::Meta::Path(path)) = attr.parse_meta() {
            if let Some(segment) = path.segments.first() {
                return segment.ident == "payable";
            }
        }
        false
    })
}

#[proc_macro_attribute]
pub fn interface(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);
    let trait_name = &input.ident;

    let methods: Vec<_> = input
        .items
        .iter()
        .map(|item| {
            if let TraitItem::Method(method) = item {
                method
            } else {
                panic!("Expected methods arguments")
            }
        })
        .collect();

    // Generate intreface implementation
    let interface = helpers::generate_interface(&methods, trait_name);

    let output = quote! {
        #interface
    };

    TokenStream::from(output)
}
