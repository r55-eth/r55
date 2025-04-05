extern crate alloc;
use alloc::vec::Vec;
use alloy_core::primitives::B256;
use core::arch::asm;
use crate::Syscall;

pub trait Event {
    fn encode_log(&self) -> (Vec<u8>, Vec<[u8; 32]>);
}

pub fn emit<T: Event>(event: T) {
    let (data, topics) = event.encode_log();
    emit_log(&data, &topics.iter().map(|t| B256::from_slice(t)).collect::<Vec<_>>());
}

pub fn emit_log(data: &[u8], topics: &[B256]) {
    let mut all_topics = [0u8; 96];
    let topics = &topics[..topics.len().min(3)];
    for (i, topic) in topics.iter().enumerate() {
        let start = i * 32;
        all_topics[start..start + 32].copy_from_slice(topic.as_ref());
    }

    log(
        data.as_ptr() as u64,
        data.len() as u64,
        all_topics.as_ptr() as u64,
        topics.len() as u64 
    );
}

pub fn log(data_ptr: u64, data_size: u64, topics_ptr: u64, topics_size: u64) {
    unsafe {
        asm!(
            "ecall",
            in("a0") data_ptr,
            in("a1") data_size,
            in("a2") topics_ptr,
            in("a3") topics_size,
            in("t0") u8::from(Syscall::Log)
        );
    }
}
