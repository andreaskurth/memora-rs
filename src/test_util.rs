// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Test utilities

use crate::error::{Error, Result};
use std::path::Path;

pub fn create_file<P: AsRef<Path>>(path: P) -> Result<std::fs::File> {
    std::fs::File::create(path.as_ref())
        .map_err(|cause| Error::chain(format!("Could not create file {:?}:", path.as_ref()), cause))
}

pub fn append_file<P: AsRef<Path>>(path: P) -> Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .append(true)
        .open(path.as_ref())
        .map_err(|cause| {
            Error::chain(
                format!("Could not open file {:?} for appending:", path.as_ref()),
                cause,
            )
        })
}

pub fn write_file(f: &mut std::fs::File, s: &str) -> Result<()> {
    use std::io::Write;
    write!(f, "{}", s)
        .map_err(|cause| Error::chain(format!("Could not write to file {:?}", f), cause))
}

/// Create symbolic link (like `ln -s <target> <link_name>`).
pub fn create_symlink<P: AsRef<Path>>(target: P, link_name: P) -> Result<()> {
    Ok(
        std::os::unix::fs::symlink(target.as_ref(), link_name.as_ref()).map_err(|cause| {
            Error::chain(
                format!("Could not create symlink {:?}:", link_name.as_ref()),
                cause,
            )
        })?,
    )
}
