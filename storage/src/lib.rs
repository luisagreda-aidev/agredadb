pub mod engine;
pub mod compression;
pub mod model;

// Re-export
pub use engine::raw_block::RawBlockManager;
pub use compression::{TurboCompressor, apply_delta_encoding, revert_delta_encoding};
pub use aligned_vec::{AVec, ConstAlign};
pub use model::PersistentNode;

pub fn init() {
    println!("Storage initialized with SIMD Turbo-Compression");
}
