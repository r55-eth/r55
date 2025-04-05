extern crate alloc;
use alloy_core::primitives::{Address, Bytes, U32};
use alloy_sol_types::{SolType, SolValue};
use ext_alloc::vec::Vec;
use core::{arch::asm, marker::PhantomData, u64};
use eth_riscv_syscalls::Syscall;

use crate::{FromBuilder, InitInterface, MethodCtx, ReadWrite};

pub trait Deployable {
    type Interface: InitInterface;

    /// Returns the contract's runtime bytecode
    fn __runtime() -> &'static [u8];

    /// Returns the contract's runtime bytecode
    fn bytecode() -> Bytes {
        Bytes::from(Self::__runtime())
    }

    // Creates a deployment builder that captures the constructor args
    fn deploy<Args>(args: Args) -> DeploymentBuilder<Self, Args> 
    where 
        Self: Sized,
        Args: SolValue + core::convert::From<<<Args as SolValue>::SolType as SolType>::RustType>
    {
        DeploymentBuilder {
            args,
            _phantom: PhantomData,
        }
    } 
}

pub struct DeploymentBuilder<D: Deployable + ?Sized, Args> 
where
    Args: SolValue + core::convert::From<<<Args as SolValue>::SolType as SolType>::RustType>
{
    args: Args,
    _phantom: PhantomData<D>,
}

impl<D: Deployable, Args> DeploymentBuilder<D, Args> 
where
    Args: SolValue + core::convert::From<<<Args as SolValue>::SolType as SolType>::RustType>
{

    // Return the interface with the appropriate context
    pub fn with_ctx<M, T>(self, ctx: M) -> T 
    where
        M: MethodCtx<Allowed = ReadWrite>, // Constrain to mutable contexts only
        D::Interface: InitInterface,
        T: FromBuilder<Context = M::Allowed>,
        D::Interface: crate::IntoInterface<T>
    {
        let bytecode = D::__runtime();
        let encoded_args = self.args.abi_encode();

        // Craft R55 initcode: [0xFF][codesize][bytecode][constructor_args]
        let codesize = U32::from(bytecode.len());

        let mut init_code = Vec::new();
        init_code.push(0xff);
        init_code.extend_from_slice(&Bytes::from(codesize.to_be_bytes_vec()));
        init_code.extend_from_slice(&bytecode);
        init_code.extend_from_slice(&encoded_args);

        let offset = init_code.as_ptr() as u64;
        let size = init_code.len() as u64;

        // TODO: think of an ergonomic API to handle deployments with values
        create(0, offset, size);

        // Get deployment address
        let mut ret_data = Vec::with_capacity(20);
        ret_data.resize(20 as usize, 0);
        return_create_address(ret_data.as_ptr() as u64);

        let address = Address::from_slice(&ret_data);
        
        // Create the interface builder
        let builder = D::Interface::new(address);
        
        // Convert to the actual interface with context
        builder.with_ctx(ctx)
    }
}

fn create(value: u64, data_offset: u64, data_size: u64) {
    unsafe {
        asm!(
            "ecall",
            in("a0") value, in("a1") data_offset, in("a2") data_size,
            in("t0") u8::from(Syscall::Create)
        );
    }
}

fn return_create_address(data_offset: u64) {
    unsafe {
        asm!(
            "ecall", in("a0") data_offset, in("t0") u8::from(Syscall::ReturnCreateAddress));
    }
}
