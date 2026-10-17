// SPDX-License-Identifier: AGPL-3.0-or-later

//! gRPC definitions between `heron-api` and `heron-engine`
//! (`proto/heron/engine/v1/engine.proto`).

/// Version 1 of the engine control interface.
pub mod engine {
    #![allow(clippy::all, clippy::pedantic, missing_docs)]
    tonic::include_proto!("heron.engine.v1");
}
