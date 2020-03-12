#![feature(never_type)]

// modules
pub mod error;
pub mod global_state;
pub mod protocol_data;
pub mod protocol_data_store;
pub mod store;
pub mod transaction_source;
pub mod trie;
pub mod trie_store;

#[cfg(test)]
use lazy_static::lazy_static;

pub(crate) const GAUGE_METRIC_KEY: &str = "gauge";
const MAX_DBS: u32 = 2;

#[cfg(test)]
lazy_static! {
    static ref TEST_MAP_SIZE: usize = {
        // Default test map size should be around ~10MiB which is also default uses LMDB by default.
        // We choose this default value to also be able to observe MapFull/MapResized error conditions under a load.
        let page_size = engine_shared::os::get_page_size().unwrap();
        page_size * 2560
    };
}
