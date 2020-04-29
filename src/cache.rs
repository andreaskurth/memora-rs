// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Build Artifact Cache

use crate::error::{Error, Result};
use crate::git::{Object, Repo};
use file_lock::FileLock;
use log::{debug, error, trace, warn};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::string::String;
use tuple_transpose::TupleTranspose;

/// A build artifact.
#[derive(Deserialize, Debug, Clone)]
pub struct Artifact {
    /// Paths of the Artifact inputs.  Each path may be a directory or a
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
}

/// Named Artifacts.  The `String` key is the name of the `Artifact` value.
///
/// ## Pattern Artifacts
/// If an Artifact name contains exactly one `%`, that artifact is a *Pattern Artifact*.  Inspired
/// by [GNU Make's Pattern
/// Rules](https://www.gnu.org/software/make/manual/html_node/Pattern-Intro.html), a Pattern
/// Artifact allows one Artifact to match multiple build artifacts with similar input and output
/// structures.  For this, the actual name given to the [`artifact` method of a
/// cache](struct.Cache.html#method.artifact) is matched against patterns.
pub type Artifacts = HashMap<String, Artifact>;

/// A build artifact cache.
#[derive(Debug)]
pub struct Cache<'a> {
    pub path: PathBuf,
    pub repo: &'a Repo,
    artifacts: &'a Artifacts, // TODO: make Artifacts owned?
}

impl<'a> Cache<'a> {
    pub fn new(path: PathBuf, repo: &'a Repo, artifacts: &'a Artifacts) -> Cache<'a> {
        Cache {
            path,
            repo,
            artifacts,
        }
    }

    fn lock_file_path(&self) -> PathBuf {
        self.path.as_path().join(".lock")
    }

    fn lock(&self, read_only: bool) -> Result<FileLock> {
        let path = {
            let path = self.lock_file_path();
            if !path.is_file() {
                debug!("Creating lock file {:?}.", path);
                fs::File::create(&path).map_err(|cause| {
                    Error::chain(format!("Could not create lockfile {:?}!", path), cause)
                })?;
            }
            match path.to_str() {
                None => Error::result(format!(
                    "Could not stringify path to lock file {:?}",
                    self.lock_file_path()
                )),
                Some(s) => Ok(String::from(s)),
            }
        }?;
        debug!("Obtaining lock ..");
        let lock = FileLock::lock(&path, true, !read_only)
            .map_err(|cause| Error::chain(format!("Could not lock {:?}!", path), cause))?;
        if read_only {
            debug!("Read-only lock obtained.");
        } else {
            debug!("Read-write lock obtained.");
        }
        Ok(lock)
    }

    fn lock_read_only(&self) -> Result<FileLock> {
        self.lock(true)
    }

    fn lock_read_write(&self) -> Result<FileLock> {
        self.lock(false)
    }

    /// Get an artifact definition by name.
    ///
    /// ## Pattern Artifact
    /// If `name` contains
    pub fn artifact(&self, name: &str) -> Result<Artifact> {
        // Match artifact names directly.
        match self.artifacts.get(name) {
            Some(a) => Ok(a.clone()), // Literal match
            None => {
                // No literal match => try pattern matches.
                // Pattern artifacts are those containing exactly one `%` in their name.
                let pattern_artifacts = self
                    .artifacts
                    .iter()
                    .filter(|(name, _)| name.matches('%').count() == 1);
                // Replace the `%` character by a word-type regex and match the given `name`.
                let mut matching_captures = pattern_artifacts
                    .filter_map(|(pattern, arti)| {
                        let pattern =
                            format!("^{}$", regex::escape(pattern).replace('%', "([[:word:]]+)"));
                        match Regex::new(&pattern) {
                            Ok(re) => re.captures(name).map(|c| (c, arti)),
                            Err(_) => None,
                        }
                    })
                    .inspect(|p| trace!("{:?}", p));

                /// Substitute the first `%` placeholder in `path` with `actual`.
                fn subst_placeholder(path: &Path, actual: &str) -> Result<PathBuf> {
                    match path.to_str() {
                        None => {
                            Error::result(format!("Could not convert path {:?} to string!", path))
                        }
                        Some(s) => Ok(PathBuf::from(s.replacen('%', actual, 1))),
                    }
                }
                // Currently, matching is only successful if there is exactly one match.
                let capture = matching_captures.next();
                match capture {
                    // No pattern matches.
                    None => Error::result(format!("Artifact \"{}\" is not defined!", name)),
                    // At least one pattern matches.
                    Some((capture, arti)) => {
                        match matching_captures.count() {
                            0 => {
                                // Exactly one pattern matches.
                                let replace_pattern = |paths: &[PathBuf]| -> Result<Vec<PathBuf>> {
                                    // Unwrap string that matched the `%` placeholder from
                                    // capture[1]. We may unwrap because we have ensured that the
                                    // capture contains group 1.
                                    let actual = capture.get(1).unwrap().as_str();
                                    paths
                                        .iter()
                                        .map(|path| subst_placeholder(path, actual))
                                        .collect()
                                };
                                (
                                    replace_pattern(&arti.inputs),
                                    replace_pattern(&arti.outputs),
                                )
                                    .transpose()
                                    .map(|(i, o)| Artifact {
                                        inputs: i,
                                        outputs: o,
                                    })
                            }
                            _ => Error::result(format!(
                                "Multiple pattern artifacts match \"{}\"!",
                                name
                            )),
                        }
                    }
                }
            }
        }
    }

    fn objects(&self) -> HashSet<Object<'a>> {
        let obj_regex = Regex::new("^[[:xdigit:]]{40}$").unwrap();
        let mut objs = HashSet::new();
        for entry in fs::read_dir(&self.path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                let dirname = path.file_name().unwrap().to_str().unwrap();
                if obj_regex.is_match(dirname) {
                    objs.insert(Object::new(dirname.to_string(), &self.repo));
                }
            }
        }
        objs
    }

    fn abspath_for_object(&self, object: &Object, subpath: &Path) -> PathBuf {
        self.path.join(&object.oid).join(subpath)
    }

    /// Determine whether a subpath exists for an object.
    pub fn subpath_in_object(&self, object: &Object, subpath: &Path) -> Option<PathBuf> {
        let abspath = self.abspath_for_object(object, subpath);
        if abspath.exists() {
            Some(abspath)
        } else {
            None
        }
    }

    /// Determine required object for artifact.
    pub fn required_object(&self, artifact: &'a Artifact) -> Option<Object<'a>> {
        // TODO: Take diff against possibly unclean working directory.
        debug!("Determining last object for each input:");
        let commits: Option<Vec<Object>> = artifact
            .inputs
            .iter()
            .map(|p| {
                let commit = self.repo.last_commit_on_path(p);
                if commit.is_some() {
                    debug!("- {:?} requires \"{}\"", p, commit.clone().unwrap());
                } else {
                    warn!("Could not determine last Git object modifying {:?}!", p);
                }
                commit
            })
            .collect();
        if commits.is_none() {
            return None;
        }
        let commits = commits.unwrap();
        let req_obj = self.repo.youngest_object(&commits);
        if req_obj.is_some() {
            debug!("Required object: \"{}\".", req_obj.unwrap());
            // FIXME: Is the lifetime of Repo for Object declared wrong?  We should be able to
            // return (a clone of) `req_obj` without the following two lines ..
            let obj = req_obj.unwrap();
            Some(Object::new(obj.oid.clone(), self.repo))
        } else {
            error!(
                "Could not determine required object for artifact {:#?}!",
                artifact
            );
            None
        }
    }

    /// Find cached object for artifact.
    pub fn cached_object(&self, artifact: &'a Artifact) -> Option<Object<'a>> {
        let req_obj = self.required_object(artifact);
        if req_obj.is_none() {
            return None;
        }
        let ancestor = req_obj.unwrap();
        let mut candidates = artifact
            .outputs
            .iter()
            .map(|oup| self.find_candidates(ancestor.clone(), oup, &artifact.inputs));
        let intersection = candidates
            .next()
            .map(|set| candidates.fold(set, |set1, set2| &set1 & &set2));
        debug!(
            "Intersection of cache candidates: {:?}, selecting one of them.",
            intersection
        );
        intersection.and_then(|set| set.iter().next().map(|obj| obj.clone()))
    }

    pub fn get(&self, artifact: &'a Artifact) -> Result<Option<Object<'a>>> {
        let _lock = self.lock_read_only()?;
        let obj = self.cached_object(artifact);
        if obj.is_none() {
            return Ok(None);
        }
        let obj = obj.unwrap();
        let path = self.path.as_path().join(&obj.oid);
        debug!("Cache path: {:?}.", path);
        for oup in &artifact.outputs {
            let src = path.as_path().join(oup);
            let dst = self.repo.path.as_path().join(oup);
            match crate::fs::copy(&src, &dst) {
                Ok(()) => (),
                Err(e) => {
                    return Err(e);
                }
            }
        }
        debug!("Releasing lock."); // TODO: Move this to `Drop` of custom lock trait.
        Ok(Some(obj))
    }

    pub fn insert(&self, artifact: &'a Artifact) -> Result<(bool, Object<'a>)> {
        let _lock = self.lock_read_write()?;
        let cached_obj = self.cached_object(artifact);
        if cached_obj.is_some() {
            return Ok((false, cached_obj.unwrap()));
        }
        let req_obj = match self.required_object(artifact) {
            None => Error::result(format!(
                "Could not determine insertion object for {:?}",
                artifact
            )),
            Some(o) => Ok(o),
        }?;
        let path = self.path.as_path().join(&req_obj.oid);
        debug!("Cache path: {:?}.", path);
        for oup in &artifact.outputs {
            let src = self.repo.path.as_path().join(oup);
            let dst = path.as_path().join(oup);
            match crate::fs::copy(&src, &dst) {
                Ok(()) => (),
                Err(e) => {
                    return Err(e);
                }
            }
        }
        debug!("Releasing lock."); // TODO: Move this to `Drop` of custom lock trait.
        Ok((true, req_obj))
    }

    /// Find the cache objects that (all of the following)
    /// - contain `subpath`
    /// - are descendants of `ancestor` (or `parent` itself)
    /// - correspond to a commit that does not modify `subpath` since `parent`.
    fn find_candidates(
        &self,
        ancestor: Object<'a>,
        subpath: &Path,
        inputs: &Vec<PathBuf>,
    ) -> HashSet<Object<'a>> {
        debug!(
            "Finding candidates for {:?} with ancestor \"{}\":",
            &subpath, &ancestor
        );
        // Simplest case: the required path exists for the ancestor itself.
        let direct_path = self.subpath_in_object(&ancestor, subpath);
        if direct_path.is_some() {
            debug!("Ancestor itself is a candidate.");
            let mut set = HashSet::new();
            set.insert(ancestor);
            return set;
        }
        trace!("Ancestor itself is not a candidate.");
        // Otherwise, we try to find another entry in the cache that matches the requirements.
        // Start with all objects in the cache.
        let objs: HashSet<Object<'a>> = self.objects();
        let candidates: HashSet<Object<'a>> = objs
            .iter()
            // Reduce to descendant objects.
            .filter(|obj| obj.is_descendant_of(&ancestor))
            .inspect(|obj| trace!("Descendant: \"{}\"", obj))
            // Reduce to objects that contain the subpath.
            .filter(|obj| self.subpath_in_object(&obj, subpath).is_some())
            .inspect(|obj| trace!("Containing subpath: \"{}\"", obj))
            // Reduce to objects that do not change any of the inputs.
            .filter(|obj| {
                let mut identical = true;
                for inp in inputs {
                    if !obj.path_is_same_as(&ancestor, inp) {
                        identical = false;
                        break;
                    }
                }
                identical
            })
            .inspect(|obj| trace!("Does not change any input: \"{}\"", obj))
            .map(|o| o.clone())
            .collect();
        debug!(
            "Candidates are: {:?}.",
            candidates
                .iter()
                .map(|obj| format!("{}", obj))
                .collect::<Vec<String>>()
        );
        candidates
    }
}
