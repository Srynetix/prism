//! The back end of the example module `hello`: its block, written as a block of Prism
//! (`hello.rs`), built into a WebAssembly component with `prism-module`.

/// The block's files reach Prism as `crate::blocks::sdk`.
pub mod blocks {
    pub use prism_module::sdk;
}

mod hello;

static BLOCKS: &[&dyn prism_module::Block] = &[&hello::HELLO];
prism_module::module!(BLOCKS);
