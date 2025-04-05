extern crate alloc;
use alloc::vec::Vec;
use core::arch::asm;
use crate::Syscall;

pub trait Error {
    fn abi_encode(&self) -> Vec<u8>;
    fn abi_decode(bytes: &[u8], validate: bool) -> Self;
}

pub fn revert() -> ! { revert_with_error(Vec::new().as_slice()) }
pub fn revert_with_error(data: &[u8]) -> ! {
    let (offset, size) = (data.as_ptr() as u64, data.len() as u64);
    unsafe {
        asm!("ecall",
            in("a0") offset, in("a1") size,
            in("t0") u8::from(Syscall::Revert));
    }
    unreachable!()
}
