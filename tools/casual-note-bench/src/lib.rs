//! Experiments on the real engine (SPK-01, SPK-05). Not product code, and nothing depends on it.
#![allow(missing_debug_implementations, clippy::needless_range_loop)]

pub mod corpus;
pub mod forks;
pub mod labelled;
pub mod rng;
pub mod searchbench;
pub mod sim;

#[cfg(feature = "wasm")]
mod web;
