use std::error::Error;

use alloy_core::primitives::keccak256;
use alloy_dyn_abi::DynSolType;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    FnArg, Ident, ImplItemMethod, LitStr, PathArguments, ReturnType, TraitItemMethod, Type,
};

// Unified method info from `ImplItemMethod` and `TraitItemMethod`
#[derive(Clone)]
pub struct MethodInfo<'a> {
    name: &'a Ident,
    args: Vec<syn::FnArg>,
    return_type: &'a ReturnType,
}

impl<'a> From<&'a ImplItemMethod> for MethodInfo<'a> {
    fn from(method: &'a ImplItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

impl<'a> From<&'a TraitItemMethod> for MethodInfo<'a> {
    fn from(method: &'a TraitItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

impl<'a> MethodInfo<'a> {
    pub fn is_mutable(&self) -> bool {
        match self.args.first() {
            Some(FnArg::Receiver(receiver)) => receiver.mutability.is_some(),
            Some(FnArg::Typed(_)) => panic!("First argument must be self"),
            None => panic!("Expected `self` as the first arg"),
        }
    }
}

// Helper function to get the parameter names + types of a method
fn get_arg_props<'a>(
    skip_first_arg: bool,
    method: &'a MethodInfo<'a>,
) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    method
        .args
        .iter()
        .skip(if skip_first_arg { 1 } else { 0 })
        .enumerate()
        .map(|(i, arg)| {
            if let FnArg::Typed(pat_type) = arg {
                (format_ident!("arg{}", i), &*pat_type.ty)
            } else {
                panic!("Expected typed arguments");
            }
        })
        .unzip()
}

pub fn get_arg_props_skip_first<'a>(
    method: &'a MethodInfo<'a>,
) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    get_arg_props(true, method)
}

pub fn get_arg_props_all<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    get_arg_props(false, method)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceNamingStyle {
    CamelCase,
}

pub struct InterfaceArgs {
    pub rename: Option<InterfaceNamingStyle>,
}

impl Parse for InterfaceArgs {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let rename_style = if !input.is_empty() {
            let value = if input.peek(LitStr) {
                input.parse::<LitStr>()?.value()
            } else {
                input.parse::<Ident>()?.to_string()
            };

            match value.as_str() {
                "camelCase" => Some(InterfaceNamingStyle::CamelCase),
                invalid => {
                    return Err(syn::Error::new(
                        input.span(),
                        format!(
                            "unsupported style: {}. Only 'camelCase' is supported",
                            invalid
                        ),
                    ))
                }
            }
        } else {
            None
        };

        Ok(InterfaceArgs {
            rename: rename_style,
        })
    }
}

// Helper function to generate interface impl from user-defined methods
pub fn generate_interface<T>(
    methods: &[&T],
    interface_name: &Ident,
    interface_style: Option<InterfaceNamingStyle>,
) -> quote::__private::TokenStream
where
    for<'a> MethodInfo<'a>: From<&'a T>,
{
    let methods: Vec<MethodInfo> = methods.iter().map(|&m| MethodInfo::from(m)).collect();
    let (mut_methods, immut_methods): (Vec<MethodInfo>, Vec<MethodInfo>) =
        methods.into_iter().partition(|m| m.is_mutable());

    // Generate implementations
    let mut_method_impls = mut_methods
        .iter()
        .map(|method| generate_method_impl(method, interface_style, true));
    let immut_method_impls = immut_methods
        .iter()
        .map(|method| generate_method_impl(method, interface_style, false));

    quote! {
        use core::marker::PhantomData;
        pub struct #interface_name<C: CallCtx> {
            address: Address,
            _ctx: PhantomData<C>
        }

        impl InitInterface for #interface_name<ReadOnly> {
            fn new(address: Address) -> InterfaceBuilder<Self> {
                InterfaceBuilder {
                    address,
                    _phantom: PhantomData
                }
            }
        }

        // Implement conversion between interface types
        impl<C: CallCtx> IntoInterface<#interface_name<C>> for #interface_name<ReadOnly> {
            fn into_interface(self) -> #interface_name<C> {
                #interface_name {
                    address: self.address,
                    _ctx: PhantomData
                }
            }
        }

        impl<C: CallCtx> FromBuilder for #interface_name<C> {
            type Context = C;

            fn from_builder(builder: InterfaceBuilder<Self>) -> Self {
                Self {
                    address: builder.address,
                    _ctx: PhantomData
                }
            }
        }

        impl <C: CallCtx> #interface_name<C> {
            pub fn address(&self) -> Address {
                self.address
            }
        }

        impl<C: StaticCtx> #interface_name<C> {
            #(#immut_method_impls)*
        }

        impl<C: MutableCtx> #interface_name<C> {
            #(#mut_method_impls)*
        }
    }
}

fn generate_method_impl(
    method: &MethodInfo,
    interface_style: Option<InterfaceNamingStyle>,
    is_mutable: bool,
) -> TokenStream {
    let name = method.name;
    let return_type = method.return_type;
    let method_selector = u32::from_be_bytes(
        generate_fn_selector(method, interface_style).expect("Unable to generate fn selector"),
    );

    let (arg_names, arg_types) = get_arg_props_skip_first(method);

    let calldata = if arg_names.is_empty() {
        quote! {
            let mut complete_calldata = Vec::with_capacity(4);
            complete_calldata.extend_from_slice(&[
                #method_selector.to_be_bytes()[0],
                #method_selector.to_be_bytes()[1],
                #method_selector.to_be_bytes()[2],
                #method_selector.to_be_bytes()[3],
            ]);
        }
    } else {
        quote! {
            let mut args_calldata = (#(#arg_names),*).abi_encode();
            let mut complete_calldata = Vec::with_capacity(4 + args_calldata.len());
            complete_calldata.extend_from_slice(&[
                #method_selector.to_be_bytes()[0],
                #method_selector.to_be_bytes()[1],
                #method_selector.to_be_bytes()[2],
                #method_selector.to_be_bytes()[3],
            ]);
            complete_calldata.append(&mut args_calldata);
        }
    };

    let (call_fn, self_param) = if is_mutable {
        (
            quote! { eth_riscv_runtime::call_contract },
            quote! { &mut self },
        )
    } else {
        (
            quote! { eth_riscv_runtime::staticcall_contract },
            quote! { &self},
        )
    };

    // Generate different implementations based on return type
    match extract_wrapper_types(&method.return_type) {
        // If `Result<T, E>` handle each individual type
        WrapperType::Result(ok_type, err_type) => quote! {
            pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Result<#ok_type, #err_type>  {
                use alloy_sol_types::SolValue;
                use alloc::vec::Vec;

                #calldata

                let result = #call_fn(
                    self.address,
                    0_u64,
                    &complete_calldata,
                    None
                );

                match <#ok_type>::abi_decode(&result, true) {
                    Ok(decoded) => Ok(decoded),
                    Err(_) => Err(<#err_type>::abi_decode(&result, true))
                }
            }
        },
        // If `Option<T>` unwrap the type to decode, and wrap it back
        WrapperType::Option(return_ty) => {
            quote! {
                pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Option<#return_ty> {
                    use alloy_sol_types::SolValue;
                    use alloc::vec::Vec;

                    #calldata

                    let result = #call_fn(
                        self.address,
                        0_u64,
                        &complete_calldata,
                        None
                    );

                    match <#return_ty>::abi_decode(&result, true) {
                        Ok(decoded) => Some(decoded),
                        Err(_) => None
                    }
                }
            }
        }
        // Otherwise, simply decode the value + wrap it in an `Option` to force error-handling
        WrapperType::None => {
            let return_ty = match return_type {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };
            quote! {
                pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Option<#return_ty> {
                    use alloy_sol_types::SolValue;
                    use alloc::vec::Vec;

                    #calldata

                    let result = #call_fn(
                        self.address,
                        0_u64,
                        &complete_calldata,
                        None
                    );

                    match <#return_ty>::abi_decode(&result, true) {
                        Ok(decoded) => Some(decoded),
                        Err(_) => None
                    }
                }
            }
        }
    }
}

pub enum WrapperType {
    Result(TokenStream, TokenStream),
    Option(TokenStream),
    None,
}

// Helper function to extract Result or Option types if present
pub fn extract_wrapper_types(return_type: &ReturnType) -> WrapperType {
    let type_path = match return_type {
        ReturnType::Default => return WrapperType::None,
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(type_path) => type_path,
            _ => return WrapperType::None,
        },
    };

    let last_segment = match type_path.path.segments.last() {
        Some(segment) => segment,
        None => return WrapperType::None,
    };

    match last_segment.ident.to_string().as_str() {
        "Result" => {
            let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
                return WrapperType::None;
            };

            let type_args: Vec<_> = args.args.iter().collect();
            if type_args.len() != 2 {
                return WrapperType::None;
            }

            // Convert the generic arguments to TokenStreams directly
            let ok_type = match &type_args[0] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            let err_type = match &type_args[1] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            WrapperType::Result(ok_type, err_type)
        }
        "Option" => {
            let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
                return WrapperType::None;
            };

            let type_args: Vec<_> = args.args.iter().collect();
            if type_args.len() != 1 {
                return WrapperType::None;
            }

            // Convert the generic argument to TokenStream
            let inner_type = match &type_args[0] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            WrapperType::Option(inner_type)
        }
        _ => WrapperType::None,
    }
}

// Helper function to generate fn selector
pub fn generate_fn_selector(
    method: &MethodInfo,
    style: Option<InterfaceNamingStyle>,
) -> Option<[u8; 4]> {
    let name = match style {
        None => method.name.to_string(),
        Some(style) => match style {
            InterfaceNamingStyle::CamelCase => to_camel_case(method.name.to_string()),
        },
    };

    let (_, arg_types) = get_arg_props_skip_first(method);
    let args = arg_types
        .iter()
        .map(|ty| rust_type_to_sol_type(ty))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let args_str = args
        .iter()
        .map(|ty| ty.sol_type_name().into_owned())
        .collect::<Vec<_>>()
        .join(",");

    let selector = format!("{}({})", name, args_str);
    let selector_bytes = keccak256(selector.as_bytes())[..4].try_into().ok()?;
    Some(selector_bytes)
}

// Helper function to convert rust types to their solidity equivalent
// TODO: make sure that the impl is robust, so far only tested with "simple types"
pub fn rust_type_to_sol_type(ty: &Type) -> Result<DynSolType, &'static str> {
    match ty {
        Type::Path(type_path) => {
            let path = &type_path.path;
            let segment = path.segments.last().ok_or("Empty type path")?;
            let ident = &segment.ident;
            let type_name = ident.to_string();

            match type_name.as_str() {
                // Fixed-size types
                "Address" => Ok(DynSolType::Address),
                "Function" => Ok(DynSolType::Function),
                "bool" | "Bool" => Ok(DynSolType::Bool),
                "String" | "str" => Ok(DynSolType::String),
                "Bytes" => Ok(DynSolType::Bytes),
                // Fixed-size bytes
                b if b.starts_with('B') => {
                    let size: usize = b
                        .trim_start_matches('B')
                        .parse()
                        .map_err(|_| "Invalid fixed bytes size")?;
                    if size > 0 && size <= 32 {
                        Ok(DynSolType::FixedBytes(size))
                    } else {
                        Err("Invalid fixed bytes size (between 1-32)")
                    }
                }
                // Fixed-size unsigned integers
                u if u.starts_with('U') => {
                    let size: usize = u
                        .trim_start_matches('U')
                        .parse()
                        .map_err(|_| "Invalid uint size")?;
                    if size > 0 && size <= 256 && size % 8 == 0 {
                        Ok(DynSolType::Uint(size))
                    } else {
                        Err("Invalid uint size (multiple of 8 + leq 256)")
                    }
                }
                // Fixed-size signed integers
                i if i.starts_with('I') => {
                    let size: usize = i
                        .trim_start_matches('I')
                        .parse()
                        .map_err(|_| "Invalid int size")?;
                    if size > 0 && size <= 256 && size % 8 == 0 {
                        Ok(DynSolType::Int(size))
                    } else {
                        Err("Invalid int size (must be multiple of 8, max 256)")
                    }
                }
                // Handle vecs
                _ => {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        match type_name.as_str() {
                            "Vec" => {
                                let inner = args.args.first().ok_or("Empty Vec type argument")?;
                                if let syn::GenericArgument::Type(inner_ty) = inner {
                                    let inner_sol_type = rust_type_to_sol_type(inner_ty)?;
                                    Ok(DynSolType::Array(Box::new(inner_sol_type)))
                                } else {
                                    Err("Invalid Vec type argument")
                                }
                            }
                            _ => Err("Unsupported generic type"),
                        }
                    } else {
                        Err("Unsupported type")
                    }
                }
            }
        }
        Type::Array(array) => {
            let inner_sol_type = rust_type_to_sol_type(&array.elem)?;
            if let syn::Expr::Lit(lit) = &array.len {
                if let syn::Lit::Int(size) = &lit.lit {
                    let size: usize = size
                        .base10_digits()
                        .parse()
                        .map_err(|_| "Invalid array size")?;
                    Ok(DynSolType::FixedArray(Box::new(inner_sol_type), size))
                } else {
                    Err("Invalid array size literal")
                }
            } else {
                Err("Invalid array size expression")
            }
        }
        Type::Tuple(tuple) => {
            let inner_types = tuple
                .elems
                .iter()
                .map(rust_type_to_sol_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DynSolType::Tuple(inner_types))
        }
        _ => Err("Unsupported type"),
    }
}

fn to_camel_case(s: String) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;

    // Iterate through characters, skipping non-alphabetic separators
    for (i, c) in s.chars().enumerate() {
        if c.is_alphanumeric() {
            if i == 0 {
                result.push(c.to_ascii_lowercase());
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        } else {
            // Set flag to capitalize next char  with non-alphanumeric ones
            capitalize_next = true;
        }
    }

    result
}

// Helper function to generate the deployment code
pub fn generate_deployment_code(
    struct_name: &Ident,
    constructor: Option<&ImplItemMethod>,
) -> quote::__private::TokenStream {
    // Decode constructor args + trigger constructor logic
    let constructor_code = match constructor {
        Some(method) => {
            let method_info = MethodInfo::from(method);
            let (arg_names, arg_types) = get_arg_props_all(&method_info);
            quote! {
                impl #struct_name { #method }

                // Get encoded constructor args
                let calldata = eth_riscv_runtime::msg_data();

                let (#(#arg_names),*) = <(#(#arg_types),*)>::abi_decode(&calldata, true)
                    .expect("Failed to decode constructor args");
                #struct_name::new(#(#arg_names),*);
            }
        }
        None => quote! {
            #struct_name::default();
        },
    };

    quote! {
        use alloc::vec::Vec;
        use alloy_core::primitives::U32;

        #[no_mangle]
        pub extern "C" fn main() -> ! {
            #constructor_code

            // Return runtime code
            let runtime: &[u8] = include_bytes!("../target/riscv64imac-unknown-none-elf/release/runtime");
            let mut prepended_runtime = Vec::with_capacity(1 + runtime.len());
            prepended_runtime.push(0xff);
            prepended_runtime.extend_from_slice(runtime);

            let prepended_runtime_slice: &[u8] = &prepended_runtime;
            let result_ptr = prepended_runtime_slice.as_ptr() as u64;
            let result_len = prepended_runtime_slice.len() as u64;
            eth_riscv_runtime::return_riscv(result_ptr, result_len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    struct MockMethod {
        method: ImplItemMethod,
    }

    impl MockMethod {
        fn new(name: &str, args: Vec<&str>) -> Self {
            let name_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let args_tokens = if args.is_empty() {
                quote!()
            } else {
                let args = args.iter().map(|arg| {
                    let parts: Vec<&str> = arg.split(": ").collect();
                    let arg_name = syn::Ident::new(parts[0], proc_macro2::Span::call_site());
                    let type_str = parts[1];
                    let type_tokens: proc_macro2::TokenStream = type_str.parse().unwrap();
                    quote!(#arg_name: #type_tokens)
                });
                quote!(, #(#args),*)
            };

            let method: ImplItemMethod = parse_quote! {
                fn #name_ident(&self #args_tokens) {}
            };
            Self { method }
        }

        fn info(&self) -> MethodInfo {
            MethodInfo::from(self)
        }
    }

    impl<'a> From<&'a MockMethod> for MethodInfo<'a> {
        fn from(test_method: &'a MockMethod) -> Self {
            MethodInfo::from(&test_method.method)
        }
    }

    pub fn get_selector_from_sig(sig: &str) -> [u8; 4] {
        keccak256(sig.as_bytes())[0..4]
            .try_into()
            .expect("Selector should have exactly 4 bytes")
    }

    #[test]
    fn test_rust_to_sol_basic_types() {
        let test_cases = vec![
            (parse_quote!(Address), DynSolType::Address),
            (parse_quote!(Function), DynSolType::Function),
            (parse_quote!(bool), DynSolType::Bool),
            (parse_quote!(Bool), DynSolType::Bool),
            (parse_quote!(String), DynSolType::String),
            (parse_quote!(str), DynSolType::String),
            (parse_quote!(Bytes), DynSolType::Bytes),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }
    }

    #[test]
    fn test_rust_to_sol_fixed_bytes() {
        let test_cases = vec![
            (parse_quote!(B1), DynSolType::FixedBytes(1)),
            (parse_quote!(B16), DynSolType::FixedBytes(16)),
            (parse_quote!(B32), DynSolType::FixedBytes(32)),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }

        // Invalid cases
        assert!(rust_type_to_sol_type(&parse_quote!(B0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(B33)).is_err());
    }

    #[test]
    fn test_rust_to_sol_integers() {
        let test_cases = vec![
            (parse_quote!(U8), DynSolType::Uint(8)),
            (parse_quote!(U256), DynSolType::Uint(256)),
            (parse_quote!(I8), DynSolType::Int(8)),
            (parse_quote!(I256), DynSolType::Int(256)),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }

        // Invalid cases
        assert!(rust_type_to_sol_type(&parse_quote!(U0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(U257)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(U7)).is_err()); // Not multiple of 8
        assert!(rust_type_to_sol_type(&parse_quote!(I0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(I257)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(I7)).is_err()); // Not multiple of 8
    }

    #[test]
    fn test_rust_to_sol_arrays() {
        // Dynamic arrays (Vec)
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<U256>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Uint(256)))
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<Bool>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Bool))
        );

        // Fixed-size arrays
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([U256; 5])).unwrap(),
            DynSolType::FixedArray(Box::new(DynSolType::Uint(256)), 5)
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([Bool; 3])).unwrap(),
            DynSolType::FixedArray(Box::new(DynSolType::Bool), 3)
        );
    }

    #[test]
    fn test_rust_to_sol_tuples() {
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((U256, Bool))).unwrap(),
            DynSolType::Tuple(vec![DynSolType::Uint(256), DynSolType::Bool])
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((Address, B32, I128))).unwrap(),
            DynSolType::Tuple(vec![
                DynSolType::Address,
                DynSolType::FixedBytes(32),
                DynSolType::Int(128)
            ])
        );
    }

    #[test]
    fn test_rust_to_sol_nested_types() {
        // Nested Vec
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<Vec<U256>>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Array(Box::new(DynSolType::Uint(256)))))
        );

        // Nested fixed array
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([[U256; 2]; 3])).unwrap(),
            DynSolType::FixedArray(
                Box::new(DynSolType::FixedArray(Box::new(DynSolType::Uint(256)), 2)),
                3
            )
        );

        // Nested tuple
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((U256, (Bool, Address)))).unwrap(),
            DynSolType::Tuple(vec![
                DynSolType::Uint(256),
                DynSolType::Tuple(vec![DynSolType::Bool, DynSolType::Address])
            ])
        );
    }

    #[test]
    fn test_rust_to_sol_invalid_types() {
        // Invalid type names
        assert!(rust_type_to_sol_type(&parse_quote!(InvalidType)).is_err());

        // Invalid generic types
        assert!(rust_type_to_sol_type(&parse_quote!(Option<U256>)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(Result<U256>)).is_err());
    }

    #[test]
    fn test_fn_selector() {
        // No arguments
        let method = MockMethod::new("balance", vec![]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("balance()"),
        );

        // Single argument
        let method = MockMethod::new("transfer", vec!["to: Address"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("transfer(address)"),
        );

        // Multiple arguments
        let method = MockMethod::new(
            "transfer_from",
            vec!["from: Address", "to: Address", "amount: U256"],
        );
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("transfer_from(address,address,uint256)")
        );

        // Dynamic arrays
        let method = MockMethod::new("batch_transfer", vec!["recipients: Vec<Address>"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("batch_transfer(address[])")
        );

        // Tuples
        let method = MockMethod::new(
            "complex_transfer",
            vec!["data: (Address, U256)", "check: (Vec<Address>, Vec<Bool>)"],
        );
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("complex_transfer((address,uint256),(address[],bool[]))")
        );

        // Fixed arrays
        let method = MockMethod::new("multi_transfer", vec!["amounts: [U256; 3]"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("multi_transfer(uint256[3])")
        );
    }

    #[test]
    fn test_fn_selector_rename_camel_case() {
        let method = MockMethod::new("get_balance", vec![]);
        assert_eq!(
            generate_fn_selector(&method.info(), Some(InterfaceNamingStyle::CamelCase)).unwrap(),
            get_selector_from_sig("getBalance()")
        );

        let method = MockMethod::new("transfer_from_account", vec!["to: Address"]);
        assert_eq!(
            generate_fn_selector(&method.info(), Some(InterfaceNamingStyle::CamelCase)).unwrap(),
            get_selector_from_sig("transferFromAccount(address)")
        );
    }

    #[test]
    fn test_fn_selector_erc20() {
        let cases = vec![
            ("totalSupply", vec![], "totalSupply()"),
            ("balanceOf", vec!["account: Address"], "balanceOf(address)"),
            (
                "transfer",
                vec!["recipient: Address", "amount: U256"],
                "transfer(address,uint256)",
            ),
            (
                "allowance",
                vec!["owner: Address", "spender: Address"],
                "allowance(address,address)",
            ),
            (
                "approve",
                vec!["spender: Address", "amount: U256"],
                "approve(address,uint256)",
            ),
            (
                "transferFrom",
                vec!["sender: Address", "recipient: Address", "amount: U256"],
                "transferFrom(address,address,uint256)",
            ),
        ];

        for (name, args, signature) in cases {
            let method = MockMethod::new(name, args);
            assert_eq!(
                generate_fn_selector(&method.info(), None).unwrap(),
                get_selector_from_sig(signature),
                "Selector mismatch for {}",
                signature
            );
        }
    }

    #[test]
    fn test_fn_selector_erc721() {
        let cases = vec![
            (
                "safeTransferFrom",
                vec![
                    "from: Address",
                    "to: Address",
                    "tokenId: U256",
                    "data: Bytes",
                ],
                "safeTransferFrom(address,address,uint256,bytes)",
            ),
            ("name", vec![], "name()"),
            ("symbol", vec![], "symbol()"),
            ("tokenURI", vec!["tokenId: U256"], "tokenURI(uint256)"),
            (
                "approve",
                vec!["to: Address", "tokenId: U256"],
                "approve(address,uint256)",
            ),
            (
                "setApprovalForAll",
                vec!["operator: Address", "approved: bool"],
                "setApprovalForAll(address,bool)",
            ),
        ];

        for (name, args, signature) in cases {
            let method = MockMethod::new(name, args);
            assert_eq!(
                generate_fn_selector(&method.info(), None).unwrap(),
                get_selector_from_sig(signature),
                "Selector mismatch for {}",
                signature
            );
        }
    }
}
