// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Git API

use crate::error::{Error, Result};
use crate::util::trim_newline;
use derivative::Derivative;
use log::{trace, warn};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A Git object identifier.
pub type Oid = String; // TODO: better Oid?

/// A Git repository.
#[derive(Derivative)]
#[derivative(PartialEq, Hash, Eq, Debug)]
pub struct Repo {
    pub path: PathBuf,
    #[derivative(PartialEq = "ignore", Hash = "ignore", Debug = "ignore")]
    ancestry_cache: RefCell<HashMap<(Oid, Oid), bool>>,
    submodule_paths: Vec<PathBuf>,
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
        let mut repo = Repo {
            path,
            ancestry_cache: RefCell::new(HashMap::new()),
            submodule_paths: vec![],
        };
        // Read submodule paths from `.gitmodules`.
        repo.submodule_paths = {
            let submodule_path_strs = repo
                .cmd_output(&[
                    "config",
                    "--file",
                    &path_str(&repo.path.join(".gitmodules")),
                    "--get-regexp",
                    r"submodule\..*\.path",
                ])
                .map_or(vec![], |oup: String| {
                    let lines: Vec<String> = oup.split('\n').map(|s| s.to_owned()).collect();
                    lines
                        .into_iter()
                        .map(|line| line.split_whitespace().nth(1).map(|s| s.to_owned()))
                        .flatten()
                        .collect::<Vec<_>>()
                });
            submodule_path_strs
                .into_iter()
                .map(|s| {
                    let p = PathBuf::from(s);
                    match p.as_path().is_relative() {
                        true => repo.path.join(p),
                        false => p,
                    }
                })
                .collect()
        };
        repo
    }

    fn custom_cmd(&self, cmd: &str, args: &[&str]) -> Command {
        let mut tmp = Command::new(cmd);
        tmp.current_dir(&self.path);
        for a in args {
            tmp.arg(a);
        }
        let cmd_str = format!("{} {}", cmd, args.join(" "));
        trace!("{}", cmd_str);
        tmp
    }

    /// Creates a Git command on this repository.
    pub fn cmd(&self, args: &[&str]) -> Command {
        self.custom_cmd("git", args)
    }

    /// Returns the standard output of a Git command on this repository if the command succeeds.
    /// Returns `None` if the command completes with non-zero exit code.
    pub fn cmd_output(&self, params: &[&str]) -> Option<String> {
        if params.len() == 0 {
            unreachable!("`cmd_output' invoked without parameters!");
        }
        let (cmd, output) = {
            let mut cmd = self.cmd(params);
            let output = cmd
                .output()
                .expect(&format!("could not get output of `{:#?}'!", cmd));
            (cmd, output)
        };
        trace!("{:?}", output);
        if output.status.success() {
            Some(trim_newline(String::from_utf8(output.stdout).expect(
                &format!("output of `{:#?}' contains non-UTF8 characters!", cmd),
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

    /// Returns true if a path contains uncommitted changes.  Returns false if the path has no
    /// uncommitted changes or has not been added to the repository.
    pub fn has_uncommitted_changes(&self, path: &Path) -> bool {
        let ls_files = self
            .cmd(&["ls-files", "-z", "--", path_str(path)])
            .stdout(std::process::Stdio::piped())
            .spawn();
        if ls_files.is_err() {
            // `git ls-files` failed.  Print a warning and continue as if the path contains uncommitted changes.
            warn!(
                "git ls-files {:?} failed, assuming path has uncommitted changes",
                path
            );
            return true;
        }
        let ls_files = ls_files.unwrap();
        if self
            .custom_cmd("xargs", &["-0", "git", "update-index", "--refresh", "--"])
            .stdin(ls_files.stdout.unwrap())
            .output()
            .is_err()
        {
            // `git update-index` failed, which means an update is needed for this path.  Treat
            // this as if the path contains uncommitted changes.
            return true;
        }
        match self
            .cmd(&["diff-index", "--quiet", "HEAD", "--", path_str(path)])
            .output()
            .map(|oup| oup.status.success())
        {
            Ok(b) => !b,    // success if no differences, so we have to invert
            Err(_) => true, // treat errors as uncommitted changes (err on the safe side)
        }
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

    /// Determine the oldest common descendant of a set of objects on the current branch.  Returns
    /// an error if any two of the objects do not have a common descendant.
    pub fn oldest_common_descendant_on_current_branch<'a>(
        &'a self,
        objects: &'a HashSet<Object<'a>>,
    ) -> Result<Object<'a>> {
        if objects.len() == 0 {
            return Error::result("no objects given");
        }
        let youngest_object = self.youngest_object(&objects);
        if youngest_object.is_ok() {
            return youngest_object.map(|obj| obj.clone());
        }
        let mut descendants = objects.iter().map(|obj| {
            obj.descendants_on_current_branch()
                .iter()
                .map(|obj| Object::new(obj.oid.clone(), &self))
                .collect::<HashSet<_>>()
        });
        let intersection: HashSet<Object> = descendants
            .next()
            .map(|set| descendants.fold(set, |set1, set2| &set1 & &set2))
            .unwrap_or_default();
        let oldest_descendant = self.oldest_object(&intersection);
        oldest_descendant.map(|obj| Object::new(obj.oid.clone(), &self))
    }

    fn object_is_ancestor_of(&self, ancestor: &Object, other: &Object) -> bool {
        let key = (ancestor.oid.clone(), other.oid.clone());
        if let Some(entry) = self.ancestry_cache.borrow().get(&key) {
            return *entry;
        }
        let output = self.cmd_output(&[
            "rev-list",
            "--ancestry-path",
            &format!("{}..{}", ancestor.oid, other.oid),
        ]);
        let entry = match output {
            None => false,
            Some(s) => s.len() > 0,
        };
        self.ancestry_cache.borrow_mut().insert(key, entry);
        entry
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
        self.repo.object_is_ancestor_of(&self, obj)
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
    use crate::fs::create_dir;
    use crate::test_util::{append_file, create_file, write_file};
    use maplit::hashset;
    use tempdir::TempDir;

    /// Test helper methods for a Git repository.
    impl Repo {
        fn cmd_assert(&self, params: &[&str]) {
            assert!(
                self.cmd_output(params).is_some(),
                "git {}",
                params.join(" ")
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

    fn repo_config_user(repo: &Repo) {
        repo.cmd_assert(&["config", "--local", "user.name", "Test"]);
        repo.cmd_assert(&["config", "--local", "user.email", "test@localhost"]);
    }

    fn setup() -> Result<(Repo, TempDir)> {
        let tmp = TempDir::new("memora-test-git")
            .map_err(|cause| Error::chain("Could not create temporary directory:", cause))?;
        let repo = Repo::new(tmp.path().to_path_buf());
        repo.cmd_assert(&["init"]);
        repo_config_user(&repo);
        Ok((repo, tmp))
    }

    fn setup_with_file(rel_path: &str) -> Result<(Repo, TempDir, std::fs::File)> {
        let (repo, tmp_dir) = setup()?;
        let fp = tmp_dir.path().join(rel_path);
        let file = create_file(fp)?;
        Ok((repo, tmp_dir, file))
    }

    struct RepoWithSubmodule {
        /// Outer repository
        outer_repo: Repo,
        /// Temporary directory of outer repository
        outer_dir: TempDir,
        /// Submodule repository cloned inside the outer repository
        submodule_repo: Repo,
        /// Path of submodule cloned inside the outer repository
        submodule_path: PathBuf,
        /// Upstream submodule repository, i.e., outside the outer repository
        _upstream_submodule_repo: Repo,
        /// Temporary directory of upstream submodule repository
        _upstream_submodule_dir: TempDir,
    }

    impl RepoWithSubmodule {
        fn setup() -> Result<RepoWithSubmodule> {
            let mut rng = rand::thread_rng();
            // Create outer repository.
            let (outer_repo, outer_dir) = setup()?;
            // Create upstream repository with one commit, so it can be cloned.
            let (_upstream_submodule_repo, upstream_submodule_dir) =
                setup_with_commits_on_file(&rand_string(&mut rng, 8), 1)?;
            // Add submodule inside outer repository.
            let submodule_name = rand_string(&mut rng, 8);
            let submodule_path = outer_dir.path().join(&submodule_name);
            outer_repo.cmd_assert(&[
                "submodule",
                "add",
                "--",
                path_str(upstream_submodule_dir.path()),
                path_str(&submodule_path),
            ]);
            outer_repo.cmd_assert(&[
                "commit",
                "-m",
                &format!("Add submodule {}", &submodule_name),
            ]);
            // Initialize cloned submodule.
            let submodule_repo = Repo::new(submodule_path.clone());
            repo_config_user(&submodule_repo);
            // Recreate object for outer repository, because we have added a submodule after its creation.
            let outer_repo = Repo::new(outer_dir.path().to_owned());
            Ok(RepoWithSubmodule {
                outer_repo,
                outer_dir,
                submodule_repo,
                submodule_path,
                _upstream_submodule_repo,
                _upstream_submodule_dir: upstream_submodule_dir,
            })
        }
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
        repo.cmd_assert(&["checkout", "main"]);
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

    #[test]
    fn oldest_common_descendant_on_current_branch_with_merge() -> Result<()> {
        let (repo, _tmp_dir) = setup_with_commits_on_file("some_file", 1)?;
        repo.cmd_assert(&["checkout", "-b", "some_branch"]);
        rand_commits_on_file(&repo, "some_file", 2)?;
        let branch_commit = repo.past_commit(1).unwrap();
        repo.cmd_assert(&["checkout", "main"]);
        rand_commits_on_file(&repo, "another_file", 20)?;
        let main_commit = repo.past_commit(10).unwrap();
        repo.cmd_assert(&["merge", "--no-edit", "some_branch"]);
        let merge_commit = repo.last_commit().unwrap();
        rand_commits_on_file(&repo, "some_file", 1)?;
        assert_eq!(
            repo.oldest_common_descendant_on_current_branch(&hashset! {branch_commit.clone(),
            main_commit.clone()})
                .unwrap(),
            merge_commit
        );
        Ok(())
    }

    #[test]
    fn uncommitted_change_in_file() -> Result<()> {
        let (repo, tmp_dir) = setup_with_commits_on_file("some_file", 1)?;
        let path = tmp_dir.path().join("some_file");
        assert_eq!(repo.has_uncommitted_changes(&path), false);
        let mut file = append_file(&path)?;
        write_file(&mut file, "bla")?;
        assert_eq!(repo.has_uncommitted_changes(&path), true);
        Ok(())
    }

    #[test]
    fn uncommitted_change_in_dir() -> Result<()> {
        let (repo, tmp_dir) = setup()?;
        let dir_path = tmp_dir.path().join("some_dir");
        create_dir(&dir_path)?;
        let file_path = dir_path.join("some_file");
        create_file(&file_path)?;
        repo.cmd_assert(&["add", "some_dir/some_file"]);
        repo.cmd_assert(&["commit", "-m", "'Add some file'"]);
        assert_eq!(repo.has_uncommitted_changes(&dir_path), false);
        let mut file = append_file(&file_path)?;
        write_file(&mut file, "foo")?;
        assert_eq!(repo.has_uncommitted_changes(&dir_path), true);
        Ok(())
    }

    #[test]
    fn submodule_path() -> Result<()> {
        let mut rws = RepoWithSubmodule::setup()?;
        // Assert that absolute path is detected.
        assert_eq!(
            rws.outer_repo.submodule_paths,
            vec![rws.submodule_path.clone()]
        );
        // Modify to relative path and make sure that is converted to an absolute path.
        rws.outer_repo.cmd_assert(&[
            "config",
            "--file",
            path_str(&rws.outer_dir.path().join(".gitmodules")),
            &format!("submodule.{}.path", path_str(&rws.submodule_path)),
            rws.submodule_path.file_name().unwrap().to_str().unwrap(),
        ]);
        rws.outer_repo = Repo::new(rws.outer_dir.path().to_owned());
        assert_eq!(
            rws.outer_repo.submodule_paths,
            vec![rws.submodule_path.clone()]
        );
        Ok(())
    }
}
