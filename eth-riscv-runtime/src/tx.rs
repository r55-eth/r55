use alloy_core::primitives::{Address, U256};
use core::arch::asm;
use eth_riscv_syscalls::Syscall;

// Returns the gas price of the current transaction
pub fn gas_price() -> U256 {
    let first: u64;
    let second: u64;
    let third: u64;
    let fourth: u64;
    unsafe { asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u8::from(Syscall::GasPrice)) }
    U256::from_limbs([first, second, third, fourth])
}

// Returns sender of the transaction (full call chain)
pub fn origin() -> Address {
    let first: u64;
    let second: u64;
    let third: u64;
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, in("t0") u8::from(Syscall::Origin));
    }
    let mut bytes = [0u8; 20];
    bytes[0..8].copy_from_slice(&first.to_be_bytes());
    bytes[8..16].copy_from_slice(&second.to_be_bytes());
    bytes[16..20].copy_from_slice(&third.to_be_bytes()[..4]);
    Address::from_slice(&bytes)
}