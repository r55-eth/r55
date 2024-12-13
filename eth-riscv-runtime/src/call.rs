#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use alloy_core::primitives::{Address, Bytes, B256};

pub fn call_contract(addr: Address, value: u64, data: &[u8], ret_size: u64) -> Option<Bytes> {
    let mut ret_data = Vec::with_capacity(ret_size as usize);
    ret_data.resize(ret_size as usize, 0);

    crate::call(
        addr,
        value,
        data.as_ptr() as u64,
        data.len() as u64,
        ret_data.as_ptr() as u64,
        ret_size,
    );

    Some(Bytes::from(ret_data))
}
