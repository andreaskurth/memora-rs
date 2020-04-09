# Memora: Build Artifact Cache for Git Repositories

[![Crate on crates.io](https://img.shields.io/crates/v/memora)](https://crates.io/crates/memora)
![Licensed MIT/Apache-2.0](https://img.shields.io/crates/l/memora)

Memora is a [build artifact][] [cache][] for [Git repositories][].  Memora's purpose is to minimize
redundant builds to save time and improve reproducibility.

Memora does *not* implement its own storage solution but relies on an existing storage system
(currently simply a locally mounted file system).  Support for other storage systems can be added on
a per-demand basis.

Memora is currently designed for use in a CI flow, but there are plans to extend it for use in the
main development flow (e.g., to swap build artifacts as one switches Git branches), for sharing
build artifacts among developers (e.g., colleagues can easily retrieve your build artifacts as they
switch to your branch), as well as for delivering build artifacts to end users.


## Installation

[Install Rust](https://doc.rust-lang.org/book/ch01-01-installation.html), then install Memora with
```sh
$ cargo install memora
```


## Usage

Memora requires a manifest file that defines the location of the cache and the artifacts of the
repository.  The manifest file *must* be named `Memora.yml` and be located in the root directory of
the Git repository or in the `.ci/` or `.gitlab-ci.d/` subfolders (earlier mention takes
precedence).  The manifest format is as follows:
```yaml
cache_root_dir: /some/path # can be absolute or relative to the root of the Git repository
artifacts:
  a: # an arbitrary name, used as `artifact` argument to the Memora subcommands
    inputs: # a list of input paths, relative to the root of the Git repository; can be files or
      - a   # directories
      - b
    outputs:    # a list of output paths, relative to the root of the Git repository; can be files
      - build/a # or directories.  Globbing is currently not supported but planned to be added.
      - build/b
```

After that, make sure the path specified under `cache_root_dir` exists and is read-/and writable by
the user executing Memora.

To obtain an artifact from the cache, execute `memora get <artifact name>` (e.g., `memora get a` in
the example manifest).  This command will return zero if the cache contains the outputs of `a` from
a revision where the inputs have not changed with respect to the current head of the Git repository.
If the cache does not meet these requirements or an error occurred (e.g., I/O), the command will
return non-zero.

To insert an artifact into the cache, execute `memora insert <artifact name>` (e.g.,
`memora insert a` in the example manifest).  This command will return zero if the outputs could be
inserted into the cache or the cache already contains the outputs (under the matching conditions
described above).  If an error occurred (e.g., I/O), the command will return non-zero.

You might want to use Memora in CI jobs like in the following example, where `compiler` is only
rebuilt if needed, otherwise obtained from the cache:
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

[build artifact]: https://en.wikipedia.org/wiki/Software_repository#Artifacts_and_packages
[cache]: https://en.wikipedia.org/wiki/Cache_(computing)
[Git repositories]: https://git-scm.com/
