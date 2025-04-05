extern crate proc_macro;
use alloy_core::primitives::U256;
use alloy_sol_types::SolValue;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, ImplItem, ImplItemMethod,
    ItemImpl, ItemTrait, ReturnType, TraitItem,
};

mod helpers;
use crate::helpers::{InterfaceArgs, MethodInfo};

#[proc_macro_derive(Error)]
pub fn error_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let variants = if let Data::Enum(data) = &input.data {
        &data.variants
    } else {
        panic!("`Error` must be an enum");
    };

    // Generate error encoding for each variant
    let encode_arms = variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        let signature = match &variant.fields {
            Fields::Unit => {
                format!("{}::{}", name, variant_name)
            }
            Fields::Unnamed(fields) => {
                let type_names: Vec<_> = fields
                    .unnamed
                    .iter()
                    .map(|f| {
                        helpers::rust_type_to_sol_type(&f.ty)
                            .expect("Unknown type")
                            .sol_type_name()
                            .into_owned()
                    })
                    .collect();

                format!("{}::{}({})", name, variant_name, type_names.join(","))
            }
            Fields::Named(_) => panic!("Named fields are not supported"),
        };

        let pattern = match &variant.fields {
            Fields::Unit => quote! { #name::#variant_name },
            Fields::Unnamed(fields) => {
                let vars: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| format_ident!("_{}", i))
                    .collect();
                quote! { #name::#variant_name(#(#vars),*) }
            }
            Fields::Named(_) => panic!("Named fields are not supported"),
        };

        // non-unit variants must encode the data
        let data = match &variant.fields {
            Fields::Unit => quote! {},
            Fields::Unnamed(fields) => {
                let vars = (0..fields.unnamed.len()).map(|i| format_ident!("_{}", i));
                quote! { #( res.extend_from_slice(&#vars.abi_encode()); )* }
            }
            Fields::Named(_) => panic!("Named fields are not supported"),
        };

        quote! {
            #pattern => {
                let mut res = Vec::new();
                let selector = keccak256(#signature.as_bytes())[..4].to_vec();
                res.extend_from_slice(&selector);
                #data
                res
            }
        }
    });

    // Generate error decoding for each variant
    let decode_arms = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        
        let signature = match &variant.fields {
            Fields::Unit => {
                format!("{}::{}", name, variant_name)
            },
            Fields::Unnamed(fields) => {
                let type_names: Vec<_> = fields.unnamed.iter()
                    .map(|f| helpers::rust_type_to_sol_type(&f.ty)
                        .expect("Unknown type")
                        .sol_type_name()
                        .into_owned()
                    ).collect();
                
                format!("{}::{}({})",
                    name,
                    variant_name,
                    type_names.join(",")
                )
            },
            Fields::Named(_) => panic!("Named fields are not supported"),
        };

        let selector_bytes = quote!{ &keccak256(#signature.as_bytes())[..4].to_vec() };

        match &variant.fields {
            Fields::Unit => quote! { selector if selector == #selector_bytes => #name::#variant_name },
            Fields::Unnamed(fields) => {
                let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let indices: Vec<_> = (0..fields.unnamed.len()).collect();
                quote!{ selector if selector == #selector_bytes => {
                    let mut values = Vec::new();
                    #( values.push(<#field_types>::abi_decode(data.unwrap(), true).expect("Unable to decode")); )*
                    #name::#variant_name(#(values[#indices]),*)
                }} 
            },
            Fields::Named(_) => panic!("Named fields are not supported"),
        }
    });

    // Generate `Debug` implementation for each variant
    let debug_arms = variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unit => quote! {
                #name::#variant_name => { f.write_str(stringify!(#variant_name)) }
            },
            Fields::Unnamed(fields) => {
                let vars: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| format_ident!("_{}", i))
                    .collect();
                quote! {
                    #name::#variant_name(#(#vars),*) => {
                        f.debug_tuple(stringify!(#variant_name))
                            #(.field(#vars))*
                            .finish()
                    }
                }
            }
            Fields::Named(_) => panic!("Named fields are not supported"),
        }
    });

    let expanded = quote! {
        impl eth_riscv_runtime::error::Error for #name {
            fn abi_encode(&self) -> alloc::vec::Vec<u8> {
                use alloy_core::primitives::keccak256;
                use alloc::vec::Vec;

                match self { #(#encode_arms),* }
            }

            fn abi_decode(bytes: &[u8], validate: bool) -> Self {
                use alloy_core::primitives::keccak256;
                use alloy_sol_types::SolValue;
                use alloc::vec::Vec;

                if bytes.len() < 4 { panic!("Invalid error length") };
                let selector = &bytes[..4];
                let data = if bytes.len() > 4 { Some(&bytes[4..]) } else { None };

                match selector {
                    #(#decode_arms),*,
                    _ => panic!("Unknown error")
                }
            }
        }

        impl core::fmt::Debug for #name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self { #(#debug_arms),* }
            }
        }
    };

    TokenStream::from(expanded)
}

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

    let mut constructor = None;
    let mut public_methods: Vec<&ImplItemMethod> = Vec::new();

    // Iterate over the items in the impl block to find pub methods + constructor
    for item in input.items.iter() {
        if let ImplItem::Method(method) = item {
            if method.sig.ident == "new" {
                constructor = Some(method);
            } else if let syn::Visibility::Public(_) = method.vis {
                public_methods.push(method);
            }
        }
    }

    let input_methods: Vec<_> = public_methods
        .iter()
        .map(|method| quote! { #method })
        .collect();
    let match_arms: Vec<_> = public_methods.iter().map(|method| {
        let method_name = &method.sig.ident;
        let method_info = MethodInfo::from(*method);
        let method_selector = u32::from_be_bytes(
            helpers::generate_fn_selector(&method_info, None)
                .expect("Unable to generate fn selector")
        );
        let (arg_names, arg_types) = helpers::get_arg_props_skip_first(&method_info);

        // Check if there are payable methods
        let checks = if !is_payable(&method) {
            quote! {
                if eth_riscv_runtime::msg_value() > U256::from(0) {
                    panic!("Non-payable function");
                }
            }
        } else {
            quote! {}
        };

        // Check if the method has a return type
        let return_handling = match &method.sig.output {
            ReturnType::Default => {
                // No return value
                quote! { self.#method_name(#( #arg_names ),*); }
            }
           ReturnType::Type(_,_) => {
                match helpers::extract_wrapper_types(&method.sig.output) {
                    helpers::WrapperType::Result(_,_) => quote! {
                        let res = self.#method_name(#( #arg_names ),*);
                        match res {
                            Ok(success) => {
                                let result_bytes = success.abi_encode();
                                let result_size = result_bytes.len() as u64;
                                let result_ptr = result_bytes.as_ptr() as u64;
                                eth_riscv_runtime::return_riscv(result_ptr, result_size);
                            }
                            Err(err) => {
                                eth_riscv_runtime::revert_with_error(&err.abi_encode());
                            }
                        }
                    },
                    helpers::WrapperType::Option(_) => quote! {
                        match self.#method_name(#( #arg_names ),*) {
                            Some(success) => {
                                let result_bytes = success.abi_encode();
                                let result_size = result_bytes.len() as u64;
                                let result_ptr = result_bytes.as_ptr() as u64;
                                eth_riscv_runtime::return_riscv(result_ptr, result_size);
                            },
                            None => eth_riscv_runtime::revert(),
                        }
                    },
                    helpers::WrapperType::None => quote! {
                        let result = self.#method_name(#( #arg_names ),*);
                        let result_bytes = result.abi_encode();
                        let result_size = result_bytes.len() as u64;
                        let result_ptr = result_bytes.as_ptr() as u64;
                        eth_riscv_runtime::return_riscv(result_ptr, result_size);
                    }
                }
            }
        };

        quote! {
            #method_selector => {
                let (#( #arg_names ),*) = <(#( #arg_types ),*)>::abi_decode(calldata, true).expect("abi decode failed");
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
    let interface = helpers::generate_interface(
        &public_methods,
        &interface_name,
        None,
    );

    // Generate initcode for deployments
    let deployment_code = helpers::generate_deployment_code(struct_name, constructor);

    // Generate the complete output with module structure
    let output = quote! {
        use eth_riscv_runtime::*;
        use alloy_sol_types::SolValue;

        // Deploy module
        #[cfg(feature = "deploy")]
            pub mod deploy {
            use super::*;
            use alloy_sol_types::SolValue;
            use eth_riscv_runtime::*;

            #emit_helper
            #deployment_code
        }

        // Public interface module
        #[cfg(not(feature = "deploy"))]
        pub mod interface {
            use super::*;
            #interface
        }

        // Generate the call method implementation privately
        // only when not in `interface-only` mode
        #[cfg(not(any(feature = "deploy", feature = "interface-only")))]
        #[allow(non_local_definitions)]
        #[allow(unused_imports)]
        #[allow(unreachable_code)]
        mod implementation {
            use super::*;
            use alloy_sol_types::SolValue;
            use eth_riscv_runtime::*;

            #emit_helper

            impl #struct_name { #(#input_methods)* }
            impl Contract for #struct_name {
                fn call(&mut self) {
                    self.call_with_data(&msg_data());
                }

                fn call_with_data(&mut self, calldata: &[u8]) {
                    let selector = u32::from_be_bytes([calldata[0], calldata[1], calldata[2], calldata[3]]);
                    let calldata = &calldata[4..];

                    match selector {
                        #( #match_arms )*
                        _ => panic!("unknown method"),
                    }

                    return_riscv(0, 0);
                }
            }

            #[eth_riscv_runtime::entry]
            fn main() -> ! {
                let mut contract = #struct_name::default();
                contract.call();
                eth_riscv_runtime::return_riscv(0, 0)
            }
        }

        // Export initcode when `deploy` mode
        #[cfg(feature = "deploy")]
        pub use deploy::*;

        // Always export the interface when not deploying
        #[cfg(not(feature = "deploy"))]
        pub use interface::*;

        // Only export contract impl when not in `interface-only` or `deploy` modes
        #[cfg(not(any(feature = "deploy", feature = "interface-only")))]
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
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);
    let args = parse_macro_input!(attr as InterfaceArgs);
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
    let interface = helpers::generate_interface(&methods, trait_name, args.rename);
    let output = quote! { #interface };

    TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn storage(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let vis = &input.vis;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                let output = quote! {
                    #vis struct #name;
                    impl #name { pub fn new() -> Self { Self {} } }
                };
                return TokenStream::from(output);
            }
        },
        _ => panic!("Storage derive only works on structs"),
    };

    // Generate the struct definition with the same fields
    let struct_fields = fields.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;
        quote! { pub #name: #ty }
    });

    // Generate initialization code for each field
    // TODO: PoC uses a naive strategy. Enhance to support complex types like tuples or custom structs.
    let init_fields = fields.iter().enumerate().map(|(i, f)| {
        let name = &f.ident;
        let slot = U256::from(i);
        let [limb0, limb1, limb2, limb3] = slot.as_limbs();
        quote! { #name: StorageLayout::allocate(#limb0, #limb1, #limb2, #limb3) }
    });

    let expanded = quote! {
        #vis struct #name { #(#struct_fields,)* }

        impl #name {
            pub fn default() -> Self {
                Self { #(#init_fields,)* }
            }
        }
    };

    TokenStream::from(expanded)
}
