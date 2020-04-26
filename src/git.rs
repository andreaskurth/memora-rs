// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

use crate::util::trim_newline;
use log::trace;
use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A Git object identifier.
type Oid = String; // TODO: better Oid?

/// A Git repository.
#[derive(PartialEq, Hash, Eq, Debug)]
pub struct Repo {
    pub path: PathBuf,
}

/// A Git object.
#[derive(PartialEq, Hash, Eq, Debug, Clone)]
pub struct Object<'a> {
    pub oid: Oid,
    pub repo: &'a Repo,
}

fn path_str(path: &Path) -> &str {
    path.to_str()
        .expect(&format!("could not convert path {:?} to string", path))
}

impl Repo {
    /// Creates a Repo object for a path.
    pub fn new(path: PathBuf) -> Repo {
        Repo { path: path }
    }

    /// Creates a Git command on this repository.
    pub fn cmd(&self, cmd: &str) -> Command {
        let mut tmp = Command::new("git");
        tmp.current_dir(&self.path);
        tmp.arg(cmd);
        tmp
    }

    /// Returns the standard output of a Git command on this repository if the command succeeds.
    /// Returns `None` if the command completes with non-zero exit code.
    pub fn cmd_output(&self, params: &[&str]) -> Option<String> {
        if params.len() == 0 {
            unreachable!("`cmd_output' invoked without parameters!");
        }
        let mut cmd = self.cmd(params[0]);
        for p in &params[1..] {
            cmd.arg(p);
        }
        let cmd_str = format!("git {}", params.join(" "));
        trace!("{}", cmd_str);
        let output = cmd
            .output()
            .expect(&format!("could not get output of `{}'!", cmd_str));
        trace!("{:?}", output);
        if output.status.success() {
            Some(trim_newline(String::from_utf8(output.stdout).expect(
                &format!("output of `{}' contains non-UTF8 characters!", cmd_str),
            )))
        } else {
            None
        }
    }

    /// Returns the last commit modifying `path`.  Returns `None` if there is no such commit.
    pub fn last_commit_on_path(&self, path: &Path) -> Option<Object> {
        self.cmd_output(&["log", "-n", "1", "--pretty=format:%H", "--", path_str(path)])
            .and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(Object::new(s, self))
                }
            })
    }

    /// Determine the ancestry order (Less = younger = further from root) for two objects.  Panics
    /// if the two objects do not have a common ancestry.
    pub fn object_cmp(&self, a: &Object, b: &Object) -> Ordering {
        if a == b {
            return Ordering::Equal;
        } else if a.is_descendant_of(b) {
            return Ordering::Less;
        } else if a.is_ancestor_of(b) {
            return Ordering::Greater;
        } else {
            panic!(
                "Cannot determine ancestry order between commits {:?} and {:?}",
                a, b
            );
        }
    }

    pub fn youngest_object<'a>(&'a self, objects: &'a Vec<Object<'a>>) -> Option<&'a Object> {
        if objects.len() == 0 {
            return None;
        }
        Some(objects.iter().min_by(|a, b| self.object_cmp(a, b)).unwrap())
    }
}

impl<'a> Display for Object<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.oid)
    }
}

impl<'a> Object<'a> {
    pub fn new(oid: Oid, repo: &'a Repo) -> Object<'a> {
        Object {
            oid: oid,
            repo: repo,
        }
    }

    pub fn is_ancestor_of(&self, obj: &Object) -> bool {
        if self.repo != obj.repo {
            return false;
        }
        let output = self.repo.cmd_output(&[
            "rev-list",
            "--ancestry-path",
            &format!("{}..{}", self.oid, obj.oid),
        ]);
        match output {
            None => false,
            Some(s) => s.len() > 0,
        }
    }

    pub fn is_descendant_of(&self, obj: &Object) -> bool {
        obj.is_ancestor_of(self)
    }

    pub fn path_is_same_as(&self, ancestor: &Object, path: &Path) -> bool {
        if self.repo != ancestor.repo {
            return false;
        }
        // TODO: need to relativize path?
        let output = self.repo.cmd_output(&[
            "diff",
            "--quiet",
            &format!("{}..{}", ancestor.oid, self.oid),
            "--",
            path_str(path),
        ]);
        output.is_some()
    }
}
