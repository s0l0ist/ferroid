/// Builds the gRPC client and server code for the `ferroid.proto` definition using `tonic-build`.
///
/// This code generation step processes the Protocol Buffer definitions located in the `proto`
/// directory and emits Rust modules with gRPC bindings into the crate's `OUT_DIR`.
///
/// # Byte Field Optimization
///
/// The `packed_ids` field in the `IdChunk` message is explicitly marked with
/// `.bytes(...)` to ensure it is deserialized as a `Bytes` type (from the `bytes` crate)
/// instead of the default `Vec<u8>`. This optimization:
///
/// - Avoids unnecessary memory allocations and copies
/// - Enables zero-copy deserialization of raw ID chunks
/// - Matches performance expectations for high-throughput ID streaming
///
/// # Files and Paths
///
/// - Proto file: `proto/ferroid.proto`
/// - Includes: `proto/`
///
/// # Panics
///
/// This function will `panic!` if code generation fails. For CI use or better
/// diagnostics, wrap with a proper error handler or logging.
///
/// # Usage
///
/// This function should be called as part of a build script (`build.rs`) to
/// generate gRPC service bindings during compilation.
///
/// ```shell
/// cargo build
/// ```
///
/// # Output
///
/// Generated code will be accessible in Rust via:
///
/// ```rust
/// pub mod ferroid {
///     tonic::include_proto!("ferroid");
/// }
/// ```
///
/// This module will include both gRPC service traits and message types.
///
use std::env;
use std::path::PathBuf;
fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("ferroid_descriptor.bin");

    let mut config = tonic_prost_build::Config::new();

    // Ensure packed binary field is treated as `Bytes`, not `Vec<u8>`
    config
        .bytes([".ferroid.IdChunk.packed_ids"])
        .file_descriptor_set_path(&descriptor_path);

    tonic_prost_build::configure()
        .compile_with_config(config, &["proto/ferroid.proto"], &["proto"])
        .unwrap();
}
