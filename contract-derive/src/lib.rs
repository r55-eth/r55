extern crate proc_macro;
use alloy_core::primitives::keccak256;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ImplItem, ItemImpl};
use syn::{FnArg, ReturnType};

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

    let mut public_methods = Vec::new();

    // Iterate over the items in the impl block to find pub methods
    for item in input.items.iter() {
        if let ImplItem::Method(method) = item {
            if let syn::Visibility::Public(_) = method.vis {
                public_methods.push(method.clone());
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
        macro_rules! emit {
            ($event_name:expr, $($arg:expr),*) => {{
                use alloy_sol_types::SolValue;
                use alloy_core::primitives::{keccak256, B256, U256, I256};
                use alloc::vec::Vec;
                use alloc::string::String;
                use alloc::borrow::ToOwned;
                
                let mut signature = alloc::vec![];
                signature.extend_from_slice($event_name.as_bytes());
                signature.extend_from_slice(b"(");
                
                let mut first = true;
                $(
                    if !first {
                        signature.extend_from_slice(b",");
                    }
                    first = false;
                    
                    signature.extend_from_slice(match stringify!($arg) {
                        // Address
                        s if s.contains("Address") || s.contains("address") => b"address",
                        
                        // Unsigned integers
                        s if s.contains("u8") => b"uint8",
                        s if s.contains("u16") => b"uint16",
                        s if s.contains("u32") => b"uint32",
                        s if s.contains("u64") => b"uint64",
                        s if s.contains("u128") => b"uint128",
                        s if s.contains("U256") || s.contains("uint256") => b"uint256",
                        
                        // Signed integers
                        s if s.contains("i8") => b"int8",
                        s if s.contains("i16") => b"int16",
                        s if s.contains("i32") => b"int32",
                        s if s.contains("i64") => b"int64",
                        s if s.contains("i128") => b"int128",
                        s if s.contains("I256") || s.contains("int256") => b"int256",
                        
                        // Boolean
                        s if s.contains("bool") => b"bool",
                        
                        // Bytes y FixedBytes
                        s if s.contains("B256") => b"bytes32",
                        s if s.contains("[u8; 32]") => b"bytes32",
                        s if s.contains("[u8; 20]") => b"bytes20",
                        s if s.contains("[u8; 16]") => b"bytes16",
                        s if s.contains("[u8; 8]") => b"bytes8",
                        s if s.contains("[u8; 4]") => b"bytes4",
                        s if s.contains("[u8; 1]") => b"bytes1",
                        
                        // Dynamic bytes & strings
                        s if s.contains("Vec<u8>") => b"bytes",
                        s if s.contains("String") || s.contains("str") => b"string",
                        
                        // Dynamic arrays
                        s if s.contains("Vec<Address>") => b"address[]",
                        s if s.contains("Vec<U256>") => b"uint256[]",
                        s if s.contains("Vec<bool>") => b"bool[]",
                        s if s.contains("Vec<B256>") => b"bytes32[]",
                        
                        // Ztatic arrays
                        s if s.contains("[Address; ") => b"address[]",
                        s if s.contains("[U256; ") => b"uint256[]",
                        s if s.contains("[bool; ") => b"bool[]",
                        s if s.contains("[B256; ") => b"bytes32[]",
                        // Tuples
                        s if s.contains("(Address, U256)") => b"(address,uint256)",
                        s if s.contains("(U256, bool)") => b"(uint256,bool)",
                        s if s.contains("(Address, Address)") => b"(address,address)",
                        
                        _ => b"uint64",
                    });
                )*
                
                signature.extend_from_slice(b")");
                
                let topic0 = B256::from(keccak256(&signature));
                let mut topics = alloc::vec![topic0];
                
                $(
                    let encoded = $arg.abi_encode();
                    if topics.len() < 3 {
                        topics.push(B256::from_slice(&encoded));
                    } else {
                        eth_riscv_runtime::emit_log(&encoded, &topics);
                    }
                )*
                
                if topics.len() > 1 {
                    let data = topics.pop().unwrap();
                    eth_riscv_runtime::emit_log(data.as_ref(), &topics);
                }
            }};
        }
    };

    // Generate the call method implementation
    let call_method = quote! {
        use alloy_sol_types::SolValue;
        use eth_riscv_runtime::*;

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
        fn main() -> !
        {
            let contract = #struct_name::default();
            contract.call();
            eth_riscv_runtime::return_riscv(0, 0)
        }
    };

    let output = quote! {
        #input
        #call_method
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
