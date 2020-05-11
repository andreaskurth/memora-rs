// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Filesystem utilities

use crate::error::{Error, Result};
use log::{debug, trace};
use std::fs;
use std::path::{Path, PathBuf};

/// Recursively create a directory and all of its parent components if they are missing.
pub fn create_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    trace!("Recursively creating directory {:?}.", path);
    std::fs::create_dir_all(path)
        .map_err(|cause| Error::chain(format!("Could not create directory {:?}:", path), cause))
}

/// Return the file type of a path without following symlinks.
pub fn file_type<P: AsRef<Path>>(path: P) -> Result<std::fs::FileType> {
    let path = path.as_ref();
    trace!("Determining file type of {:?}.", path);
    let metadata = path
        .symlink_metadata()
        .map_err(|cause| Error::chain(format!("Could not get metadata of {:?}:", path), cause))?;
    Ok(metadata.file_type())
}

/// Copy symlink without dereferencing it.
fn copy_symlink<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    trace!("Copying symlink {:?} to {:?}.", from, to);
    let link_target = std::fs::read_link(from).map_err(|cause| {
        Error::chain(format!("Could not read source symlink {:?}:", from), cause)
    })?;
    std::os::unix::fs::symlink(link_target, to).map_err(|cause| {
        Error::chain(
            format!("Could not create destination symlink {:?}:", to),
            cause,
        )
    })?;
    Ok(())
}

/// Recursively create parent components to a path if they are missing.
pub fn create_parents<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    trace!("Creating parents of {:?}.", path);
    let path_parent = match path.parent() {
        None => Error::result(format!(
            "Could not determine parent directory of {:?}",
            path
        )),
        Some(p) => Ok(p),
    }?;
    create_dir(&path_parent)
}

/// Recursively copy path `from` to path `to`.
///
/// - `to` must not be a subpath of `from`.
/// - If parent directories of `to` do not exist, they are created.
/// - Symlinks are not followed but copied "verbatim".
/// - Files that exist under `to` and under `from` are overwritten with the file under `from`.
/// - Files that exist under `to` but not under `from` are not touched.
pub fn copy<P, Q>(from: P, to: Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let from = from.as_ref();
    let to = to.as_ref();
    debug!("Copying {:?} to {:?}.", from, to);
    // The case when `from` itself is a symlink needs to be handled specially because `WalkDir`
    // always dereferences the given (top-level) path.
    if file_type(from)?.is_symlink() {
        trace!("From path is a symlink.");
        create_parents(to)?;
        copy_symlink(from, to)?;
    } else {
        for entry in walkdir::WalkDir::new(from).follow_links(false) {
            // Determine path to entry.
            let entry_path = entry
                .as_ref()
                .map_err(|_cause| Error::new(format!("Cannot handle filesystem entry {:?}", entry)))
                .map(|entry| entry.path())?;
            // Determine relative path of *from* entry.
            let relative_from = entry_path.strip_prefix(from).map_err(|cause| {
                Error::chain(format!("Cannot relativize path {:?}:", entry_path), cause)
            })?;
            let from = entry_path;
            // Determine absolute path of *to* entry.
            let to = if relative_from != Path::new("") {
                to.join(relative_from)
            } else {
                PathBuf::from(to)
            };
            trace!("Copying {:?} to {:?}.", from, to);
            // Create parent directory of `to` (if it does not exist).
            create_parents(&to)?;
            // Query metadata (without following symlinks).
            let filetype = file_type(&from)?;
            if filetype.is_file() {
                // Copy `from` file using standard `fs::copy`.
                fs::copy(&from, &to).map_err(|cause| {
                    Error::chain(
                        format!("Could not copy file {:?} to {:?}!", from, to),
                        cause,
                    )
                })?;
            } else if filetype.is_dir() {
                create_dir(&to)?;
            } else if filetype.is_symlink() {
                copy_symlink(&from, &to)?;
            } else {
                Error::result(format!("Can not copy file type {:?}", filetype))?;
            }
        }
    }
    Ok(())
}
