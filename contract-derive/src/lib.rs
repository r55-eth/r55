extern crate proc_macro;
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

    let match_arms: Vec<_> = public_methods.iter().enumerate().map(|(index, method)| {
        let method_name = &method.sig.ident;
        let method_selector = index as u32;
        let arg_types: Vec<_> = method.sig.inputs.iter().skip(1).map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                let ty = &*pat_type.ty;
                quote! { #ty }
            } else {
                panic!("Expected typed arguments");
            }
        }).collect();

        let arg_names: Vec<_> = (0..method.sig.inputs.len() - 1).map(|i| format_ident!("arg{}", i)).collect();

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
                #return_handling
            }
        }
    }).collect();

    let emit_helper = quote! {
        fn emit<T: alloy_sol_types::SolEvent>(event: T) {
            use alloy_sol_types::private::B256;
            
            let log_data = event.encode_log_data();
            let topics = event.encode_topics();
            let num_topics = topics.len();
            
            let mut all_topics = [0u8; 96]; 
            for (i, topic) in topics.iter().enumerate() {
                if i >= 3 { break; } 
                let start = i * 32;
                let topic_bytes: B256 = (*topic).into();
                all_topics[start..start + 32].copy_from_slice(topic_bytes.as_ref());
            }
    
            log(
                log_data.data.as_ptr() as u64,
                log_data.data.len() as u64,
                all_topics.as_ptr() as u64,
                num_topics as u64  
            );
        }
    };

    // Generate the call method implementation
    let call_method = quote! {
        use alloy_sol_types::SolValue;
        use eth_riscv_runtime::{revert, msg_sender, return_riscv, slice_from_raw_parts, Contract, log};
        use alloy_sol_types::SolEvent;

        #emit_helper

        impl Contract for #struct_name {
            fn call(&self) {
                let address: usize = 0x8000_0000;
                let length = unsafe { slice_from_raw_parts(address, 8) };
                let length = u64::from_le_bytes([length[0], length[1], length[2], length[3], length[4], length[5], length[6], length[7]]) as usize;
                let calldata = unsafe { slice_from_raw_parts(address + 8, length) };
                self.call_with_data(calldata);
            }

            fn call_with_data(&self, calldata: &[u8]) {
                let selector = u32::from_le_bytes([calldata[0], calldata[1], calldata[2], calldata[3]]);
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
