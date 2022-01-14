// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

//! Command-Line Interface

use crate::cache::Cache;
use crate::config::Manifest;
use crate::error::{Error, Result};
use crate::git::Repo;
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use inflector::Inflector;
use log::{debug, info};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn main() -> Result<bool> {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let name = env!("CARGO_PKG_NAME").to_title_case();
    let version = env!("CARGO_PKG_VERSION");

    let app = App::new(&name)
    .setting(AppSettings::SubcommandRequiredElseHelp)
    .version(version)
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("A Build Artifact Cache for Git Repositories.")
    .arg(Arg::with_name("working_dir")
            .short("C")
            .takes_value(true)
            .help("Run as if started in this path.")
    )
    .arg(Arg::with_name("ignore_uncommitted_changes")
            .long("ignore-uncommitted-changes")
            .help("Ignores uncommitted changes")
    )
    .subcommand(SubCommand::with_name("get")
            .about("Get the outputs of an artifact from the cache or exit non-zero if the artifact is not cached.")
            .arg(Arg::with_name("artifact")
                    .takes_value(true)
                    .required(true)
             )
    )
    .subcommand(SubCommand::with_name("insert")
            .about("Insert the outputs of an artifact into the cache.")
            .arg(Arg::with_name("artifact")
                    .takes_value(true)
                    .required(true)
             )
    )
    .subcommand(SubCommand::with_name("lookup")
            .about("Look an artifact up in the cache.  Exit zero iff the artifact is cached.")
            .arg(Arg::with_name("artifact")
                    .takes_value(true)
                    .required(true)
             )
    );

    // Parse command-line arguments.
    let matches = app.get_matches();

    debug!("{} v{}", name, version);

    // Determine working directory.
    let working_dir: PathBuf = {
        let path_str = matches.value_of("working_dir").unwrap_or(".");
        fs::canonicalize(path_str).map_err(|cause| {
            Error::chain(
                format!(
                    "Failed to canonicalize path of working directory {:?}!",
                    path_str
                ),
                cause,
            )
        })
    }?;
    debug!("Working directory: {:?}.", working_dir);

    // Find Git repository in working directory.
    let repo: Repo = {
        let tmp = Repo::new(working_dir);
        let git_path = match tmp.cmd_output(&["rev-parse", "--show-toplevel"]) {
            None => Error::result(format!("Could not find Git repository.")),
            Some(s) => fs::canonicalize(&s).map_err(|cause| {
                Error::chain(
                    format!("Failed to canonicalize path of Git repository {:?}!", s),
                    cause,
                )
            }),
        }?;
        Repo::new(git_path)
    };
    debug!("Git repository: {:?}.", repo);

    // Find manifest in repository.
    let manifest: Manifest = {
        let manifest_path = {
            let mut iter = ["Memora.yml", ".ci/Memora.yml", ".gitlab-ci.d/Memora.yml"]
                .iter()
                .map(|s| Path::new(s))
                .map(|p| repo.path.join(p))
                .map(|p| fs::canonicalize(p))
                .filter(|r| r.is_ok())
                .map(|r| r.unwrap());
            match iter.next() {
                None => Error::result(format!("Could not find Memora manifest.")),
                Some(p) => Ok(p),
            }?
        };
        Manifest::from_path(&manifest_path)?
    };
    debug!("Memora manifest: {:?}.", manifest);

    let disabled = match manifest.disable_env_var {
        Some(e) => match env::var(&e) {
            Ok(_) => {
                info!(
                    "{} disabled because environment variable \"{}\" is set.",
                    name, e
                );
                true
            }
            _ => false,
        },
        _ => false,
    };

    // Initialize cache.
    let cache: Cache = {
        let root_dir = match manifest.cache_root_dir.is_absolute() {
            true => manifest.cache_root_dir.clone(),
            false => repo.path.join(&manifest.cache_root_dir),
        };
        let cache_path = fs::canonicalize(&root_dir).map_err(|cause| {
            Error::chain(
                format!("Failed to canonicalize path of cache {:?}!", root_dir),
                cause,
            )
        })?;
        Cache::new(cache_path, &repo, &manifest.artifacts)
    };
    debug!("Cache: {:?}.", cache);

    let ignore_uncommitted_changes = matches.is_present("ignore_uncommitted_changes");
    if ignore_uncommitted_changes {
        debug!("Ignoring uncommitted changes.");
    }

    match matches.subcommand() {
        ("get", Some(matches)) => match disabled {
            false => get(&cache, matches, ignore_uncommitted_changes),
            true => Ok(false),
        },
        ("insert", Some(matches)) => match disabled {
            false => insert(&cache, matches, ignore_uncommitted_changes),
            true => Ok(true),
        },
        ("lookup", Some(matches)) => match disabled {
            false => lookup(&cache, matches, ignore_uncommitted_changes),
            true => Ok(false),
        },
        _ => Error::result("Unknown combination of subcommand and arguments!"),
    }
}

fn artifact_name<'a>(matches: &'a ArgMatches) -> Result<&'a str> {
    match matches.value_of("artifact") {
        None => Error::result("Required \"artifact\" argument was not provided!"),
        Some(s) => Ok(s),
    }
}

pub fn get(cache: &Cache, matches: &ArgMatches, ignore_uncommitted_changes: bool) -> Result<bool> {
    let artifact_name = artifact_name(matches)?;
    let artifact = cache.artifact(artifact_name)?;
    match cache.get(&artifact, ignore_uncommitted_changes) {
        Ok(Some(obj)) => {
            info!("Got artifact \"{}\" from {:?}.", artifact_name, obj.oid);
            Ok(true)
        }
        Ok(None) => {
            info!("Artifact \"{}\" not found in cache.", artifact_name);
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

pub fn insert(
    cache: &Cache,
    matches: &ArgMatches,
    ignore_uncommitted_changes: bool,
) -> Result<bool> {
    let artifact_name = artifact_name(matches)?;
    let artifact = cache.artifact(artifact_name)?;
    match cache.insert(&artifact, ignore_uncommitted_changes) {
        Ok((false, obj)) => {
            info!(
                "Artifact artifact \"{}\" already exists under {:?}, did not insert.",
                artifact_name, obj.oid
            );
            Ok(true)
        }
        Ok((true, obj)) => {
            info!(
                "Inserted artifact \"{}\" under {:?}.",
                artifact_name, obj.oid
            );
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

pub fn lookup(
    cache: &Cache,
    matches: &ArgMatches,
    ignore_uncommitted_changes: bool,
) -> Result<bool> {
    let artifact_name = artifact_name(matches)?;
    let artifact = cache.artifact(artifact_name)?;
    match cache.cached_object(&artifact, ignore_uncommitted_changes) {
        Some(obj) => {
            info!("Found artifact \"{}\" in {:?}.", artifact_name, obj.oid);
            Ok(true)
        }
        None => {
            info!("Artifact \"{}\" not found in cache.", artifact_name);
            Ok(false)
        }
    }
}
