# Memora: Build Artifact Cache for Git Repositories

[![Crate on crates.io](https://img.shields.io/crates/v/memora)](https://crates.io/crates/memora)
![Licensed MIT/Apache-2.0](https://img.shields.io/crates/l/memora)

Memora is a [build artifact][] [cache][] for [Git repositories][].  Memora's purpose is to minimize
redundant builds to save time and improve reproducibility.

Memora is designed to be minimal and self-contained.  There are only three requirements for using
Memora: a Git repository, a [Memora manifest file](#manifest-file), and the
[`memora` executable](#installation).  Memora does not depend on or interfere with your build flow
in any way.

Memora does *not* implement its own storage solution but relies on an existing storage system
(currently simply a locally mounted file system).  Support for other storage systems could be added
on demand.

A Memora cache can be safely used by an arbitrary number of concurrently running `memora` processes.
Race conditions are prevented with [POSIX advisory record locks][].

Memora is currently designed for [use in a CI flow](#example-ci-configuration), but there are plans
to extend it for use in the main development flow (e.g., to swap build artifacts as one switches Git
branches), for sharing build artifacts among developers (e.g., colleagues can easily retrieve your
build artifacts as they switch to your branch), as well as for delivering build artifacts to end
users.


## Installation

[Install Rust](https://doc.rust-lang.org/book/ch01-01-installation.html), then install Memora with
```sh
$ cargo install memora
```


## Usage

### Manifest File

Memora requires a manifest file that defines the location of the cache and the artifacts of the
repository.  The manifest file *must* be named `Memora.yml` and be located in the root directory of
the Git repository or in the `.ci/` or `.gitlab-ci.d/` subfolders (earlier mention takes
precedence).  The manifest format is as follows:
```yaml
# This is the root directory of the build artifact cache for this Git repository.  The path can be
# absolute or relative to the root of the repository.
cache_root_dir: /some/path
# Each repository has a set of artifact definitions.
artifacts:
  # Each artifact must have a name.  This name is used as `artifact` argument to Memora
  # subcommands, so it should be kept short.  The name of an artifact must be unique among all
  # artifacts in a Memora manifest.
  foo:
    # Each artifact has a list of input and output paths.  All paths must be relative to the root of
    # the repository.  Each path points to a file or a directory.  If it points to a directory, the
    # entire directory is considered.  Wildcards/globbing are currently not supported (but planned
    # to be added for outputs).
    #
    # Inputs are the paths your build flow uses to build the outputs of an artifact.  For example,
    # this could be source code, Makefiles, or configuration files.  Each input must be checked into
    # the Git repository.  The list of inputs must be complete; that is, when none of the inputs
    # changes between two Git objects (e.g., a commit), the entire artifact is considered identical
    # for those two objects.  One input may be used in more than one artifact.  The path to the
    # used `Memora.yml` manifest is an implicit input for every artifact.
    inputs:
      - a
      - b
    # Outputs are the paths your build flow creates or modifies when it builds an artifact.  For
    # example, this could be executables or shared object files.  The list of outputs must contain
    # all files required to "use" the artifact but can (and should in most cases) omit intermediate
    # build products.
    outputs:
      - install/bin/a
      - install/lib/b
```

### Cache Directory

After that, make sure the path specified under `cache_root_dir` exists and is readable and writable
by the user executing Memora.

### Getting Artifact from Cache

To obtain an artifact from the cache, execute `memora get <artifact name>` (e.g., `memora get foo`
in the example manifest).  This command will return zero if the cache contains the outputs of `foo`
from a revision where the inputs have not changed with respect to the current head of the Git
repository.  If the cache does not meet these requirements or an error occurred (e.g., I/O), the
command will return non-zero.  If you want to know whether an artifact is cached without getting its
outputs, use `memora lookup`.

### Inserting Artifact into Cache

To insert an artifact into the cache, execute `memora insert <artifact name>` (e.g.,
`memora insert foo` in the example manifest).  This command will return zero if the outputs could be
inserted into the cache or the cache already contains the outputs (under the matching conditions
described above).  If an error occurred (e.g., I/O), the command will return non-zero.

### Example CI Configuration

You might want to use Memora in CI jobs like in the following example, where the `compiler` artifact
is only rebuilt if needed, otherwise obtained from the cache:
```yaml
build_and_run:
  script:
    - >
      if ! memora get compiler; then
        make compiler
        memora insert compiler
      fi
    - ./compile ...
```
That's it, you have cached the `compiler` artifact without any requiring any specific features of
your CI runner or management software.  If you want to disable Memora for some CI runs (e.g.,
*nightly*), set `disable_env_var` in the manifest to an environment variable that is defined during
those runs.


[build artifact]: https://en.wikipedia.org/wiki/Software_repository#Artifacts_and_packages
[cache]: https://en.wikipedia.org/wiki/Cache_(computing)
[Git repositories]: https://git-scm.com/
[POSIX advisory record locks]: https://en.wikipedia.org/wiki/File_locking#In_Unix-like_systems
