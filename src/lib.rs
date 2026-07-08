//! A general-purpose Rust crate for the Lawo Ember+ control protocol.
//!
//! This crate uses a hybrid architecture:
//! - FFI to Lawo's `libember_slim` for BER/Glow encoding and decoding.
//! - Native Rust for S101 framing, the async TCP server, and the provider state machine.

#![warn(missing_docs)]

mod sys;

pub mod error;
pub mod glow;
pub mod handler;
pub mod provider;
pub mod s101;
pub mod server;
pub mod tree;

pub use error::{Error, Result};
