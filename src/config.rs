// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

use crate::cache::Artifacts;
use crate::error::{Error, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A Memora build artifact cache manifest.
#[derive(Deserialize, Debug)]
pub struct Manifest {
    /// The root directory of the cache.
    pub cache_root_dir: PathBuf,
    /// The artifacts.
    pub artifacts: Artifacts,
    /// Optional name of an environment variable that, if set, disables the cache.
    pub disable_env_var: Option<String>,
}
impl Manifest {
    pub fn from_path(path: &Path) -> Result<Manifest> {
        use std::fs::File;
        let file = File::open(path)
            .map_err(|cause| Error::chain(format!("Cannot open manifest {:?}!", path), cause))?;
        let manifest = {
            let mut manifest: Manifest = serde_yaml::from_reader(file).map_err(|cause| {
                Error::chain(format!("Syntax error in manifest {:?}!", path), cause)
            })?;
            // Add path of Manifest to inputs of each Artifact.
            for (_, artifact) in &mut manifest.artifacts {
                artifact.inputs.push(path.to_path_buf())
            };
            manifest
        };
        Ok(manifest)
    }
}
