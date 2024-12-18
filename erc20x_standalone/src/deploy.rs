#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;

use eth_riscv_runtime::return_riscv;

#[eth_riscv_runtime::entry]
fn main() -> !
{
    //decode constructor arguments
    //constructor(ars);
    let runtime: &[u8] = include_bytes!("../target/riscv64imac-unknown-none-elf/release/runtime");

    let mut prepended_runtime = Vec::with_capacity(1 + runtime.len());
    prepended_runtime.push(0xff);
    prepended_runtime.extend_from_slice(runtime);

    let prepended_runtime_slice: &[u8] = &prepended_runtime;

    let result_ptr = prepended_runtime_slice.as_ptr() as u64;
    let result_len = prepended_runtime_slice.len() as u64;
    return_riscv(result_ptr, result_len);
}
