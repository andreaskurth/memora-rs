// Copyright 2020 Andreas Kurth
//
// SPDX-License-Identifier: (Apache-2.0 OR MIT)

use log::error;
use memora::cli;

fn main() {
    match cli::main() {
        Ok(b) => match b {
            true => std::process::exit(0),
            false => std::process::exit(1),
        },
        Err(e) => {
            error!("{}", e);
            std::process::exit(-1);
        }
    }
}
