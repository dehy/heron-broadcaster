// SPDX-License-Identifier: AGPL-3.0-or-later

//! Compiles the gRPC definitions with protox, a protobuf compiler written in
//! Rust: no `protoc` to install on build machines.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "proto/heron/engine/v1/engine.proto";
    println!("cargo:rerun-if-changed={proto}");
    let descriptors = protox::compile([proto], ["proto"])?;
    tonic_prost_build::configure().compile_fds(descriptors)?;
    Ok(())
}
