#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use alloy_core::primitives::{Address, Bytes, U256};
use core::arch::asm;
use core::marker::PhantomData;
use eth_riscv_syscalls::Syscall;

// Concrete types implementing the context traits
pub struct ReadOnly;
pub struct ReadWrite;

// Marker traits to determine call context
pub trait CallCtx {}
pub trait StaticCtx: CallCtx {}
pub trait MutableCtx: StaticCtx {}

impl CallCtx for ReadOnly {}
impl CallCtx for ReadWrite {}
impl StaticCtx for ReadOnly {}
impl StaticCtx for ReadWrite {}
impl MutableCtx for ReadWrite {}

// Marker trait to connect contract method context with call ctx
pub trait MethodCtx { type Allowed: CallCtx; }
impl<'a, T> MethodCtx for &'a T { type Allowed = ReadOnly; }
impl<'a, T> MethodCtx for &'a mut T { type Allowed = ReadWrite; }

// Types and traits to build a MethodCtx-aware interface
pub struct InterfaceBuilder<I> {
    pub address: Address,
    pub _phantom: PhantomData<I>,
}

pub trait InitInterface: Sized {
    fn new(address: Address) -> InterfaceBuilder<Self>;
}

// Key change: Add trait to convert between interface types
pub trait IntoInterface<T> {
    fn into_interface(self) -> T;
}

impl<I> InterfaceBuilder<I> {
    pub fn with_ctx<M: MethodCtx, T>(
        self,
        _: M
    ) -> T 
    where
        I: IntoInterface<T>,
        M: MethodCtx<Allowed = T::Context>,
        T: FromBuilder
    {
        let target_builder = InterfaceBuilder {
            address: self.address,
            _phantom: PhantomData,
        };
        T::from_builder(target_builder)
    }
}

pub trait FromBuilder: Sized {
    type Context: CallCtx;
    fn from_builder(builder: InterfaceBuilder<Self>) -> Self;
}


/// Trait for contracts to have an entry point for txs  
pub trait Contract {
    fn call(&mut self);
    fn call_with_data(&mut self, calldata: &[u8]);
}

pub fn call_contract(
    addr: Address,
    value: u64,
    data: &[u8],
    ret_size: Option<u64>,
) -> Option<Bytes> {
    // Perform the call without writing return data into (REVM) memory
    call(addr, value, data.as_ptr() as u64, data.len() as u64);

    let output = handle_call_output(ret_size);
    Some(output)
}

pub fn call(addr: Address, value: u64, data_offset: u64, data_size: u64) {
    let addr: U256 = addr.into_word().into();
    let addr = addr.as_limbs();
    unsafe {
        asm!(
            "ecall",
            in("a0") addr[0], in("a1") addr[1], in("a2") addr[2],
            in("a3") value, in("a4") data_offset, in("a5") data_size,
            in("t0") u8::from(Syscall::Call)
        );
    }
}

pub fn staticcall_contract(addr: Address, value: u64, data: &[u8], ret_size: Option<u64>) -> Option<Bytes> {
    // Perform the staticcall without writing return data into (REVM) memory
    staticcall(addr, value, data.as_ptr() as u64, data.len() as u64);

    let output = handle_call_output(ret_size);
    Some(output)
}

fn handle_call_output(ret_size: Option<u64>) -> Bytes {
    // Figure out return data size + initialize memory location
    let ret_size = match ret_size {
        Some(size) => size,
        None => return_data_size(),
    };
    if ret_size == 0 {
        return Bytes::default()
    };

    let mut ret_data = Vec::with_capacity(ret_size as usize);
    ret_data.resize(ret_size as usize, 0);

    // Copy the return data from the interpreter's buffer
    let (offset, chunks) = (ret_data.as_ptr() as u64, ret_size / 32);
    for i in 0..chunks {
        let step = i * 32;
        return_data_copy(offset + step, step, 32)
    };

    Bytes::from(ret_data)
}

pub fn staticcall(addr: Address, value: u64, data_offset: u64, data_size: u64) {
    let addr: U256 = addr.into_word().into();
    let addr = addr.as_limbs();
    unsafe {
        asm!(
            "ecall",
            in("a0") addr[0], in("a1") addr[1], in("a2") addr[2],
            in("a3") value, in("a4") data_offset, in("a5") data_size,
            in("t0") u8::from(Syscall::StaticCall)
        );
    }
}

pub fn return_data_size() -> u64 {
    let size: u64;
    unsafe {
        asm!( "ecall", lateout("a0") size, in("t0") u8::from(Syscall::ReturnDataSize));
    }

    size
}

pub fn return_data_copy(dest_offset: u64, res_offset: u64, res_size: u64) {
    unsafe {
        asm!(
            "ecall",
            in("a0") dest_offset, in("a1") res_offset, in("a2") res_size, in("t0")
            u8::from(Syscall::ReturnDataCopy)
        );
    }
}
