// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Git API

use crate::error::{Error, Result};
use crate::util::trim_newline;
use log::trace;
use std::cmp::Ordering;
use std::collections::HashSet;
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

    /// Returns the first of a set of objects according to a given ordering.  Returns an error if
    /// the set is empty or any two of the objects are incomparable.
    fn first_ordered_object<'a>(
        &'a self,
        objects: &'a HashSet<Object<'a>>,
        ord: Ordering,
    ) -> Result<&'a Object> {
        if objects.len() == 0 {
            return Error::result("no objects given");
        }
        if objects.len() == 1 {
            return Ok(objects.iter().next().unwrap());
        }
        let mut iter = objects.iter();
        objects
            .iter()
            .try_fold(iter.next().unwrap(), |youngest, obj| {
                match obj.partial_cmp(&youngest) {
                    Some(o) => {
                        if o == ord {
                            Ok(obj)
                        } else {
                            Ok(youngest)
                        }
                    }
                    None => Error::result(format!("{:?} and {:?} are incomparable", youngest, obj)),
                }
            })
    }

    /// Returns the youngest (= furthest from root) of a set of objects.  Returns an error if the
    /// set is empty or any two of the objects are incomparable.
    pub fn youngest_object<'a>(&'a self, objects: &'a HashSet<Object<'a>>) -> Result<&'a Object> {
        self.first_ordered_object(objects, Ordering::Less)
    }

    /// Returns the oldest (= closest to root) of a set of objects.  Returns an error if the set is
    /// empty or any of two of the objects are incomparable.
    pub fn oldest_object<'a>(&'a self, objects: &'a HashSet<Object<'a>>) -> Result<&'a Object> {
        self.first_ordered_object(objects, Ordering::Greater)
    }
}

impl<'a> Display for Object<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.oid)
    }
}

impl<'a> PartialOrd for Object<'a> {
    fn partial_cmp(&self, other: &Object) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        } else if self.is_descendant_of(other) {
            return Some(Ordering::Less);
        } else if self.is_ancestor_of(other) {
            return Some(Ordering::Greater);
        } else {
            return None; // incomparable
        }
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

    /// Get descendants of this commit on the current branch, in chronological order.
    fn descendants_on_current_branch(&self) -> Vec<Object<'a>> {
        match self.repo.cmd_output(&[
            "rev-list",
            "--ancestry-path",
            "--reverse",
            &format!("{}..", self.oid),
        ]) {
            Some(s) => s
                .lines()
                .map(|line| Object::new(line.to_string(), self.repo))
                .collect(),
            None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Error, Result};
    use maplit::hashset;
    use tempdir::TempDir;

    /// Test helper methods for a Git repository.
    impl Repo {
        fn cmd_assert(&self, params: &[&str]) {
            assert!(
                self.cmd_output(params).is_some(),
                format!("git {}", params.join(" "))
            );
        }
        fn last_commit(&self) -> Option<Object> {
            self.past_commit(0)
        }
        fn past_commit(&self, n_commits_ago: usize) -> Option<Object> {
            self.cmd_output(&["rev-parse", &format!("HEAD~{}", n_commits_ago)])
                .and_then(|oup| oup.lines().next().map(|l| l.to_string()))
                .map(|head_commit| Object::new(head_commit.to_string(), &self))
        }
    }

    fn create_file<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<std::fs::File> {
        std::fs::File::create(&path)
            .map_err(|cause| Error::chain(format!("Could not create file {:?}:", path), cause))
    }

    fn write_file(f: &mut std::fs::File, s: &str) -> Result<()> {
        use std::io::Write;
        write!(f, "{}", s)
            .map_err(|cause| Error::chain(format!("Could not write to file {:?}", f), cause))
    }

    fn setup() -> Result<(Repo, TempDir)> {
        let tmp = TempDir::new("memora-test-git")
            .map_err(|cause| Error::chain("Could not create temporary directory:", cause))?;
        let repo = Repo::new(tmp.path().to_path_buf());
        repo.cmd_assert(&["init"]);
        repo.cmd_assert(&["config", "--local", "user.name", "Test"]);
        repo.cmd_assert(&["config", "--local", "user.email", "test@localhost"]);
        Ok((repo, tmp))
    }

    fn setup_with_file(rel_path: &str) -> Result<(Repo, TempDir, std::fs::File)> {
        let (repo, tmp_dir) = setup()?;
        let fp = tmp_dir.path().join(rel_path);
        let file = create_file(fp)?;
        Ok((repo, tmp_dir, file))
    }

    fn rand_string(rng: &mut dyn rand::RngCore, n_chars: usize) -> String {
        use rand::distributions::Alphanumeric;
        use rand::Rng;
        rng.sample_iter(Alphanumeric).take(n_chars).collect()
    }

    fn rand_commits_on_file(repo: &Repo, rel_path: &str, n_commits: usize) -> Result<()> {
        let mut rng = rand::thread_rng();
        let mut file = create_file(repo.path.join(rel_path))?;
        for _i in 0..n_commits {
            write_file(&mut file, &rand_string(&mut rng, 10))?;
            repo.cmd_assert(&["add", rel_path]);
            repo.cmd_assert(&["commit", "-m", &rand_string(&mut rng, 10)]);
        }
        Ok(())
    }

    fn setup_with_commits_on_file(rel_path: &str, n_commits: usize) -> Result<(Repo, TempDir)> {
        let (repo, tmp_dir, _file) = setup_with_file(rel_path)?;
        rand_commits_on_file(&repo, rel_path, n_commits)?;
        Ok((repo, tmp_dir))
    }

    fn create_two_incomparable_commits<'a>(
        repo: &'a Repo,
        path: &str,
    ) -> Result<(Object<'a>, Object<'a>)> {
        repo.cmd_assert(&["checkout", "-b", "some_branch"]);
        rand_commits_on_file(&repo, path, 1)?;
        let some_commit = repo.last_commit().unwrap();
        repo.cmd_assert(&["checkout", "master"]);
        repo.cmd_assert(&["checkout", "-b", "another_branch"]);
        rand_commits_on_file(&repo, path, 1)?;
        let another_commit = repo.last_commit().unwrap();
        Ok((some_commit, another_commit))
    }

    #[test]
    fn last_commit_on_existing_path_with_single_commit() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 1)?;
        let act = repo.last_commit_on_path(Path::new("some_file"));
        assert_eq!(act, repo.last_commit());
        Ok(())
    }

    #[test]
    fn last_commit_on_existing_path_with_no_commit() -> Result<()> {
        let (repo, _tmp_dir, _file) = setup_with_file("some_file")?;
        let act = repo.last_commit_on_path(Path::new("some_file"));
        assert_eq!(act, None);
        Ok(())
    }

    #[test]
    fn last_commit_on_existing_path_with_two_commits() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 2)?;
        let act = repo.last_commit_on_path(Path::new("some_file"));
        assert_eq!(act, repo.last_commit());
        Ok(())
    }

    #[test]
    fn last_commit_on_nonexistent_path() -> Result<()> {
        let (repo, _tmp_dir, _file) = setup_with_file("some_file")?;
        let act = repo.last_commit_on_path(Path::new("some_other_file"));
        assert_eq!(act, None);
        Ok(())
    }

    #[test]
    fn youngest_object_no_commit() -> Result<()> {
        let (repo, _tmp_dir, _file) = setup_with_file("some_file")?;
        assert!(repo.youngest_object(&hashset! {}).is_err());
        Ok(())
    }

    #[test]
    fn youngest_object_single_commit() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 5)?;
        let obj = repo.last_commit().unwrap();
        assert_eq!(repo.youngest_object(&hashset! {obj.clone()}).unwrap(), &obj);
        Ok(())
    }

    #[test]
    fn youngest_object_two_identical_commits() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 7)?;
        let obj = repo.last_commit().unwrap();
        assert_eq!(
            repo.youngest_object(&hashset! {obj.clone(), obj.clone()})
                .unwrap(),
            &obj
        );
        Ok(())
    }

    #[test]
    fn youngest_object_two_different_commits() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 7)?;
        let younger = repo.last_commit().unwrap();
        let older = repo.past_commit(4).unwrap();
        assert_eq!(
            repo.youngest_object(&hashset! {older.clone(), younger.clone()})
                .unwrap(),
            &younger
        );
        assert_eq!(
            repo.youngest_object(&hashset! {younger.clone(), older.clone()})
                .unwrap(),
            &younger
        );
        Ok(())
    }

    #[test]
    fn youngest_object_two_incomparable_commits() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 7)?;
        let (some_commit, another_commit) = create_two_incomparable_commits(&repo, "some_file")?;
        assert!(repo
            .youngest_object(&hashset! {some_commit.clone(), another_commit.clone()})
            .is_err());
        Ok(())
    }

    #[test]
    fn partial_cmp_different_objects() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 5)?;
        let younger = repo.past_commit(1).unwrap();
        let older = repo.past_commit(4).unwrap();
        assert_eq!(younger.partial_cmp(&older), Some(Ordering::Less));
        assert_eq!(older.partial_cmp(&younger), Some(Ordering::Greater));
        Ok(())
    }

    #[test]
    fn partial_cmp_identical_objects() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 5)?;
        let younger = repo.past_commit(1).unwrap();
        assert_eq!(younger.partial_cmp(&younger), Some(Ordering::Equal));
        Ok(())
    }

    #[test]
    fn partial_cmp_incomparable_objects() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 1)?;
        let (some_commit, another_commit) = create_two_incomparable_commits(&repo, "some_file")?;
        assert_eq!(some_commit.partial_cmp(&another_commit), None);
        Ok(())
    }

    #[test]
    fn descendants_on_current_branch() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 5)?;
        let ancestor = repo.past_commit(3).unwrap();
        let descendants = {
            let mut vec = Vec::new();
            for i in (0..3).rev() {
                vec.push(repo.past_commit(i).unwrap());
            }
            vec
        };
        assert_eq!(ancestor.descendants_on_current_branch(), descendants);
        Ok(())
    }
}
