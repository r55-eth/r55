use alloy_core::primitives::U256;
use eth_riscv_syscalls::Syscall;
use core::arch::asm;

// Returns current block timestamp in seconds since Unix epoch
pub fn timestamp() -> U256 {
    let first: u64;
    let second: u64;
    let third: u64;
    let fourth: u64;
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u32::from(Syscall::Timestamp));
    }
    U256::from_limbs([first, second, third, fourth])
}

// Returns current block base fee (EIP-3198 and EIP-1559)
pub fn base_fee() -> U256 {
    let first: u64;
    let second: u64;
    let third: u64;
    let fourth: u64;
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u32::from(Syscall::BaseFee));
    }
    U256::from_limbs([first, second, third, fourth])
}

// Returns current chain ID
pub fn chain_id() -> u64 {
    let id: u64;
    unsafe {
        asm!("ecall", lateout("a0") id, in("t0") u32::from(Syscall::ChainId));
    }
    id
}

// Returns current block gas limit
pub fn gas_limit() -> U256 {
    let first: u64;
    let second: u64;
    let third: u64;
    let fourth: u64;
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u32::from(Syscall::GasLimit));
    }
    U256::from_limbs([first, second, third, fourth])
}

// Returns current block number
pub fn number() -> U256 {
    let first: u64;
    let second: u64;
    let third: u64;
    let fourth: u64;
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u32::from(Syscall::Number));
    }
    U256::from_limbs([first, second, third, fourth])
}