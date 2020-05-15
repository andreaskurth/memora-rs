# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## Unreleased

### Added

### Changed

### Fixed


## 0.4.2

### Fixed
- `cache::Cache::find_candidates` now also includes descendants when the ancestor itself is a
  candidate.


## 0.4.1

### Fixed
- `fs::copy` now also overwrites symlinks.


## 0.4.0

### Changed
- `fs::copy` no longer follows and resolves symlinks.  As a result, symlinks are now inserted and
  retrieved verbatim from the build artifact cache.  Additionally, it is now possible to cache
  artifacts with circular or nonexistent symlink targets.


## 0.3.1

### Fixed
- `git::Repo::oldest_common_descendant_on_current_branch`: Add missing intersection of descendants.


## 0.3.0

### Added
- Add support for Pattern Artifacts, which allows one Artifact definition to match many actual
  artifacts; see the [documentation of `cache::Artifacts`][PatternArtifacts] for details.
- `git::Object`: Implement `PartialOrd`.
- `git::Repo`:
  - Add `oldest_object` method as counterpart of `youngest_object`.
  - Add `oldest_common_descendant_on_current_branch` method to determine the oldest common
    descendant of a set of objects on the current branch.

### Changed
- `cache::Cache`: `artifacts` field is now private.  Get an individual artifact by name using the
  `artifact` method.  Getting all artifact definitions is no longer possible outside `Cache`.
- `git::Repo`:
  - `youngest_object` method now returns a `Result` instead of an `Option` and no longer panics if
    any two given `objects` are incomparable.
  - `youngest_object` method now takes a `HashSet` as `objects` argument, because the `objects` are
    unordered.

### Fixed
- `cache::Cache::required_object`: Fix required object resolution when merges are involved.


## 0.2.2

### Fixed

- `git::Repo::last_commit_on_path`: Return `None` when Git log for given path is empty.
- `cache::Cache::required_object`: Return `None` when required object for at least one input is
  not known.


## 0.2.1

### Fixed
- Implicitly add path of manifest to inputs of all artifacts.


## 0.2.0

### Added
- Add `lookup` command to determine whether an artifact is cached without copying the outputs.


## 0.1.1

### Fixed
- README: Add missing 'POSIX advisory record locks' URL.


## 0.1.0

Initial release.


[PatternArtifacts]: https://docs.rs/memora/latest/memora/cache/type.Artifacts.html#PatternArtifacts
