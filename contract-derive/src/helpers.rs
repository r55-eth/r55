use std::error::Error;

use alloy_core::primitives::keccak256;
use alloy_dyn_abi::DynSolType;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    FnArg, Ident, ImplItemMethod, Meta, ReturnType, TraitItemMethod, Type,
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
) -> (Vec<Ident>, Vec<&syn::Type>) {
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

pub fn get_arg_props_skip_first<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&syn::Type>) {
    get_arg_props(true, method)
}

pub fn get_arg_props_all<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&syn::Type>) {
    get_arg_props(false, method)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceCompilationTarget {
    R55,
    EVM,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceNamingStyle {
    CamelCase,
}

pub struct InterfaceArgs {
    pub target: InterfaceCompilationTarget,
    pub rename: Option<InterfaceNamingStyle>,
}

impl Parse for InterfaceArgs {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        // default arg values if uninformed
        let mut target = InterfaceCompilationTarget::R55;
        let mut rename_style = None;

        // Parse all attributes
        let mut first = true;
        while !input.is_empty() {
            if !first {
                input.parse::<syn::Token![,]>()?;
            }
            first = false;

            let key: syn::Ident = input.parse()?;

            match key.to_string().as_str() {
                "rename" => {
                    input.parse::<syn::Token![=]>()?;

                    // Handle both string literals and identifiers
                    let value = if input.peek(syn::LitStr) {
                        let lit: syn::LitStr = input.parse()?;
                        lit.value()
                    } else {
                        let ident: syn::Ident = input.parse()?;
                        ident.to_string()
                    };

                    rename_style = Some(match value.as_str() {
                        "camelCase" => InterfaceNamingStyle::CamelCase,
                        invalid => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!("unsupported rename style: {}", invalid),
                            ))
                        }
                    });
                }
                "target" => {
                    input.parse::<syn::Token![=]>()?;

                    // Handle both string literals and identifiers
                    let value = if input.peek(syn::LitStr) {
                        let lit: syn::LitStr = input.parse()?;
                        lit.value()
                    } else {
                        let ident: syn::Ident = input.parse()?;
                        ident.to_string()
                    };

                    target = match value.as_str() {
                        "r55" => InterfaceCompilationTarget::R55,
                        "evm" => InterfaceCompilationTarget::EVM,
                        invalid => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!("unsupported compilation target: {}", invalid),
                            ))
                        }
                    };
                }
                invalid => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute: {}", invalid),
                    ))
                }
            }
        }

        Ok(InterfaceArgs {
            target,
            rename: rename_style,
        })
    }
}

// Helper function to generate interface impl from user-defined methods
pub fn generate_interface<T>(
    methods: &[&T],
    interface_name: &Ident,
    interface_style: Option<InterfaceNamingStyle>,
    interface_target: InterfaceCompilationTarget,
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
        .map(|method| generate_method_impl(method, interface_target, interface_style, true));
    let immut_method_impls = immut_methods
        .iter()
        .map(|method| generate_method_impl(method, interface_target, interface_style, false));

    quote! {
        pub struct #interface_name<C: eth_riscv_runtime::StaticCallCtx> {
            address: Address,
            _ctx: PhantomData<C>
        }

        impl<C: eth_riscv_runtime::StaticCallCtx> #interface_name<C> {
            pub fn new(address: Address) -> Self {
                Self { address, _ctx: PhantomData::default() }
            }

            #(#immut_method_impls)*
        }

        impl<C: eth_riscv_runtime::MutableCallCtx> #interface_name<C> {
            #(#mut_method_impls)*
        }
    }
}

fn generate_method_impl(
    method: &MethodInfo,
    interface_target: InterfaceCompilationTarget,
    interface_style: Option<InterfaceNamingStyle>,
    is_mutable: bool,
) -> TokenStream {
    let name = method.name;
    let return_type = method.return_type;
    let method_selector = match interface_target {
        InterfaceCompilationTarget::R55 => u32::from_be_bytes(
            generate_selector_r55(method, interface_style)
                .expect("Unable to generate r55 fn selector"),
        ),
        InterfaceCompilationTarget::EVM => u32::from_be_bytes(
            generate_selector_evm(method, interface_style)
                .expect("Unable to generate evm fn selector"),
        ),
    };

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
            )?;

            <#return_ty>::abi_decode(&result, true).ok()
        }
    }
}

// Helper function to generate fn selector for r55 contracts
pub fn generate_selector_r55(
    method: &MethodInfo,
    style: Option<InterfaceNamingStyle>,
) -> Option<[u8; 4]> {
    let name = match style {
        None => method.name.to_string(),
        Some(style) => match style {
            InterfaceNamingStyle::CamelCase => to_camel_case(method.name.to_string()),
        },
    };
    keccak256(name)[..4].try_into().ok()
}

// Helper function to generate fn selector for evm contracts
pub fn generate_selector_evm(
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
    keccak256(selector)[..4].try_into().ok()
}

// Helper function to convert rust types to their solidity equivalent
// TODO: make sure that the impl is robust, so far only tested with "simple types"
fn rust_type_to_sol_type(ty: &Type) -> Result<DynSolType, &'static str> {
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
    _struct_name: &Ident,
    constructor: Option<&ImplItemMethod>,
) -> quote::__private::TokenStream {
    quote! {
        use alloc::vec::Vec;

        #[no_mangle]
        pub extern "C" fn main() -> ! {
            // TODO: figure out constructor

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
