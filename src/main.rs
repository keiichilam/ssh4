#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "console")]

mod auth;
mod cli;
mod config;
mod gui;
mod remote_fs;
mod ssh_client;
mod terminal;
mod transfer;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();

    // GUI mode: default when no host and no profile, or forced with --gui.
    if args.gui || (args.host.is_none() && args.profile.is_none()) {
        if let Err(e) = gui::run() {
            eprintln!("GUI error: {e}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(e) = cli::run(args) {
        eprintln!("ssh4: {e}");
        std::process::exit(1);
    }
}
