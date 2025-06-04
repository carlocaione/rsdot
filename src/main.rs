mod commands;
mod utils;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use git2::Repository;
use std::{env, path::PathBuf};
use utils::validate_file;

const VAULT_DIR_ENV: &str = "VAULT_DIR";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// print vault status
    Status,
    /// add files to config vault
    Add {
        /// config name
        conf_name: String,
        #[arg(value_parser = validate_file)]
        /// files to add
        files: Option<Vec<PathBuf>>,
    },
    /// commit changes
    Sync {
        #[arg(short, long)]
        /// push to remote
        push: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    let vault = match env::var(VAULT_DIR_ENV) {
        Err(_) => bail!("{} must be set", VAULT_DIR_ENV),
        Ok(vault) => {
            let vault = PathBuf::from(vault);
            if !vault.is_dir() {
                bail!("{} is not a directory", VAULT_DIR_ENV)
            }
            vault
        }
    };

    let repo = Repository::open(&vault).ok();

    match &args.cmd {
        Commands::Status => commands::status::execute(&vault, repo.as_ref())?,
        Commands::Add { conf_name, files } => commands::add::execute(&vault, conf_name, files)?,
        Commands::Sync { push } => commands::sync::execute(repo.as_ref(), *push)?,
    }

    Ok(())
}
