#![no_std]
#![no_main]
#![feature(alloc_error_handler, maybe_uninit_write_slice, round_char_boundary)]

use alloy_core::primitives::{Address, U256};
use core::{arch::asm, panic::PanicInfo, slice};
pub use riscv_rt::entry;

mod alloc;
pub mod block;
pub mod types;
pub mod tx;

pub mod err;
pub use err::{revert, revert_with_error, Error};

pub mod log;
pub use log::{emit_log, Event};

pub mod call;
pub use call::*;

const CALLDATA_ADDRESS: usize = 0x8000_0000;

pub unsafe fn slice_from_raw_parts(address: usize, length: usize) -> &'static [u8] {
    slice::from_raw_parts(address as *const u8, length)
}

#[panic_handler]
unsafe fn panic(_panic: &PanicInfo<'_>) -> ! {
    static mut IS_PANICKING: bool = false;

    if !IS_PANICKING {
        IS_PANICKING = true;

        revert();
        // TODO with string
        //print!("{panic}\n");
    } else {
        revert();
        // TODO with string
        //print_str("Panic handler has panicked! Things are very dire indeed...\n");
    }
}

use eth_riscv_syscalls::Syscall;

pub fn return_riscv(addr: u64, offset: u64) -> ! {
    unsafe {
        asm!("ecall", in("a0") addr, in("a1") offset, in("t0") u8::from(Syscall::Return));
    }
    unreachable!()
}

pub fn sload(key: U256) -> U256 {
    let key = key.as_limbs();
    let (val0, val1, val2, val3): (u64, u64, u64, u64);
    unsafe {
        asm!(
            "ecall",
            lateout("a0") val0, lateout("a1") val1, lateout("a2") val2, lateout("a3") val3,
            in("a0") key[0], in("a1") key[1], in("a2") key[2], in("a3") key[3],
            in("t0") u8::from(Syscall::SLoad));
    }
    U256::from_limbs([val0, val1, val2, val3])
}

pub fn sstore(key: U256, value: U256) {
    let key = key.as_limbs();
    let value = value.as_limbs();

    unsafe {
        asm!(
            "ecall",
            in("a0") key[0], in("a1") key[1], in("a2") key[2], in("a3") key[3],
            in("a4") value[0], in("a5") value[1], in("a6") value[2], in("a7") value[3],
            in("t0") u8::from(Syscall::SStore)
        );
    }
}

pub fn keccak256(offset: u64, size: u64) -> U256 {
    let (first, second, third, fourth): (u64, u64, u64, u64);
    unsafe {
        asm!(
            "ecall",
            in("a0") offset,
            in("a1") size,
            lateout("a0") first,
            lateout("a1") second,
            lateout("a2") third,
            lateout("a3") fourth,
            in("t0") u8::from(Syscall::Keccak256)
        );
    }
    U256::from_limbs([first, second, third, fourth])
}

pub fn msg_sender() -> Address {
    let (first, second, third): (u64, u64, u64);
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, in("t0") u8::from(Syscall::Caller));
    }
    let mut bytes = [0u8; 20];
    bytes[0..8].copy_from_slice(&first.to_be_bytes());
    bytes[8..16].copy_from_slice(&second.to_be_bytes());
    bytes[16..20].copy_from_slice(&third.to_be_bytes()[..4]);
    Address::from_slice(&bytes)
}

pub fn msg_value() -> U256 {
    let (first, second, third, fourth): (u64, u64, u64, u64);
    unsafe {
        asm!("ecall", lateout("a0") first, lateout("a1") second, lateout("a2") third, lateout("a3") fourth, in("t0") u8::from(Syscall::CallValue));
    }
    U256::from_limbs([first, second, third, fourth])
}

pub fn msg_sig() -> [u8; 4] {
    let sig = unsafe { slice_from_raw_parts(CALLDATA_ADDRESS + 8, 4) };
    sig.try_into().unwrap()
}

pub fn msg_data() -> &'static [u8] {
    let length = unsafe { slice_from_raw_parts(CALLDATA_ADDRESS, 8) };
    let length = u64::from_le_bytes([
        length[0], length[1], length[2], length[3], length[4], length[5], length[6], length[7],
    ]) as usize;
    unsafe { slice_from_raw_parts(CALLDATA_ADDRESS + 8, length) }
}

#[allow(non_snake_case)]
#[no_mangle]
fn DefaultHandler() {
    revert();
}

#[allow(non_snake_case)]
#[no_mangle]
fn ExceptionHandler(_trap_frame: &riscv_rt::TrapFrame) -> ! {
    revert();
}
