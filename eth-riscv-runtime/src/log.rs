use alloy_core::primitives::B256;

pub fn emit_log(data: &[u8], topics: &[B256]) {
    let mut all_topics = [0u8; 96];
    for (i, topic) in topics.iter().enumerate() {
        if i >= 3 { break; }
        let start = i * 32;
        all_topics[start..start + 32].copy_from_slice(topic.as_ref());
    }

    crate::log(
        data.as_ptr() as u64,
        data.len() as u64,
        all_topics.as_ptr() as u64,
        topics.len() as u64
    );
}