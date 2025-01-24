#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use alloy_core::primitives::{Address, Bytes, U256};
use eth_riscv_syscalls::Syscall;
use core::arch::asm;

pub trait Contract {
    fn call(&mut self);
    fn call_with_data(&mut self, calldata: &[u8]);
}

pub fn call_contract(addr: Address, value: u64, data: &[u8], ret_size: Option<u64>) -> Option<Bytes> {
    // Perform the call without writting return data into (REVM) memory
    call(addr, value, data.as_ptr() as u64, data.len() as u64);

    // Figure out return data size + initialize memory location
    let ret_size = match ret_size {
        Some(size) => size,
        None => return_data_size(),
    };
    if ret_size == 0 { return Some(Bytes::default())};

    let mut ret_data = Vec::with_capacity(ret_size as usize);
    ret_data.resize(ret_size as usize, 0);

    // Copy the return data from the interpreter's buffer
    let (offset, chuncks) = (ret_data.as_ptr() as u64, ret_size / 32);
    for i in 0..chuncks {
        let step = i * 32;
        return_data_copy(offset + step, step, 32)
    };

    Some(Bytes::from(ret_data))
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

pub fn return_data_size() -> u64 {
    let size: u64;
    unsafe { asm!( "ecall", lateout("a0") size, in("t0") u8::from(Syscall::ReturnDataSize)); }

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
