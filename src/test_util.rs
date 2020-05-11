// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Test utilities

use crate::error::{Error, Result};
use std::path::Path;

pub fn create_file<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<std::fs::File> {
    std::fs::File::create(&path)
        .map_err(|cause| Error::chain(format!("Could not create file {:?}:", path), cause))
}

pub fn write_file(f: &mut std::fs::File, s: &str) -> Result<()> {
    use std::io::Write;
    write!(f, "{}", s)
        .map_err(|cause| Error::chain(format!("Could not write to file {:?}", f), cause))
}

