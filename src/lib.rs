// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Memora is a build artifact cache for Git repositories.  Memora's purpose is to minimize
//! redundant builds to save time and improve reproducibility.
//!
//! Please see the README for a general introduction.

pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod fs;
pub mod git;
pub mod util;

#[cfg(test)]
pub mod test_util;
