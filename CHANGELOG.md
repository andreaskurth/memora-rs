# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## Unreleased

### Added
- Add support for Pattern Artifacts, which allows one Artifact definition to match many actual
  artifacts; see the documentation of `cache::Artifacts` for details.
- `git::Object`: Implement `PartialOrd`.

### Changed
- `cache::Cache`: `artifacts` field is now private.  Get an individual artifact by name using the
  `artifact` method.  Getting all artifact definitions is no longer possible outside `Cache`.

### Fixed


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
