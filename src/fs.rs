// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Filesystem utilities

use crate::error::{Error, Result};
use log::debug;
use std::fs;
use std::path::Path;

pub fn copy<P, Q>(from: P, to: Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let from = from.as_ref();
    let to = to.as_ref();
    debug!("Copying {:?} to {:?}", from, to);
    // Create directory containing the `to` (if it does not exist).
    let to_dir = match to.parent() {
        None => Error::result(format!("Could not determine parent directory of {:?}", to)),
        Some(p) => Ok(p),
    }?;
    fs::create_dir_all(&to_dir).map_err(|cause| {
        Error::chain(
            format!("Could not create directory containing copy target!"),
            cause,
        )
    })?;
    // Query metadata *without* following symlink.
    let metadata = from
        .symlink_metadata()
        .map_err(|cause| Error::chain(format!("Could not get metadata of {:?}", from), cause))?;
    let filetype = metadata.file_type();
    if filetype.is_dir() {
        // Copy `from` directory recursively using `fs_extra::dir::copy`.
        let copy_options = fs_extra::dir::CopyOptions {
            overwrite: true,
            copy_inside: false,
            skip_exist: false,
            buffer_size: 64000,
            depth: 0,
        };
        fs_extra::dir::copy(from, to_dir, &copy_options).map_err(|cause| {
            Error::chain(
                format!("Could not copy directory {:?} to {:?}!", from, to),
                cause,
            )
        })?;
    } else if filetype.is_file() {
        // Copy `from` file using standard `fs::copy`.
        fs::copy(from, to).map_err(|cause| {
            Error::chain(
                format!("Could not copy file {:?} to {:?}!", from, to),
                cause,
            )
        })?;
    } else if filetype.is_symlink() {
        let link_target = std::fs::read_link(from).map_err(|cause| {
            Error::chain(
                format!("Could not read source symlink {:?}:", from),
                cause,
            )
        })?;
        std::os::unix::fs::symlink(link_target, to).map_err(|cause| {
            Error::chain(
                format!("Could not create destination symlink {:?}:", to),
                cause,
            )
        })?;
    } else {
        Error::result(format!("Can not copy file type {:?}", filetype))?;
    }
    Ok(())
}
