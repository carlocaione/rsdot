use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use git2::{IndexAddOption, Repository, Status};
use inquire::Confirm;
use owo_colors::OwoColorize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[cfg(unix)]
use std::os::unix;

#[cfg(windows)]
use std::os::windows;

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

fn validate_file(file: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(file);

    if !path.exists() {
        return Err(format!("File '{}' does not exist", file));
    }

    if path.is_absolute() {
        return Err(
            "Use only paths relative to the configuration directory, not absolute".to_string(),
        );
    }

    Ok(path)
}

struct FileStatus(PathBuf, Option<Status>);

impl Commands {
    fn do_status(&self, vault: &Path, repo: Option<&Repository>) -> Result<()> {
        println!();
        println!(
            "  {} Vault location: {}",
            "→".blue(),
            vault.display().to_string().cyan()
        );

        if let Some(repo) = repo {
            println!(
                "  {} GIT repo location: {}",
                "→".blue(),
                repo.path().display().cyan()
            );

            if let Ok(head) = repo.head() {
                if let Some(branch_name) = head.shorthand() {
                    println!("  {} Current branch: {}", "→".blue(), branch_name.cyan());
                }
            }
        }
        println!();

        let mut confs = fs::read_dir(vault)
            .context("Failed to read vault directory")?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                let file_name = path.file_name()?.to_str()?;

                if path.is_dir() && !file_name.starts_with('.') {
                    Some((file_name.to_string(), path))
                } else {
                    None
                }
            })
            .collect::<Vec<(String, PathBuf)>>();

        if confs.is_empty() {
            println!("  {} No configurations found", "ℹ".blue());
            return Ok(());
        }

        confs.sort_by(|a, b| a.0.cmp(&b.0));

        for (conf_name, conf_path) in confs {
            println!("  {} {}", "→".blue(), conf_name.red().bold());

            let files: Vec<FileStatus> = WalkDir::new(&conf_path)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    let rel_path = path.strip_prefix(vault).ok()?;

                    let status = match repo {
                        None => None,
                        Some(repo) => repo.status_file(rel_path).ok(),
                    };

                    Some(FileStatus(path.to_path_buf(), status))
                })
                .skip(1)
                .collect();

            if files.is_empty() {
                println!("      {} (empty)", "·".dimmed());
            } else {
                for file_status in files {
                    print_status(file_status, &conf_path).context("Failed to print file status")?;
                }
            }

            println!();
        }

        Ok(())
    }

    fn do_add(&self, vault: &Path, conf_name: &str, files: &Option<Vec<PathBuf>>) -> Result<()> {
        let conf_path = vault.join(conf_name);

        if !conf_path.exists() {
            let should_create = Confirm::new(&format!(
                "'{}' configuration does not exist. Do you want to create it?",
                conf_name
            ))
            .with_default(false)
            .prompt()
            .context("Failed to get user confirmation")?;

            if !should_create {
                return Ok(());
            }

            fs::create_dir_all(&conf_path).context("Cannot create the configuration directory")?;

            println!(
                "  {} Created configuration: {}",
                "✓".green(),
                conf_name.cyan()
            );
        }

        let Some(files) = files else {
            println!("  {} No files specified", "ℹ".blue());
            return Ok(());
        };

        move_and_symlink(files, &conf_path).context("Failed to move and symlink files")?;

        Ok(())
    }

    fn do_sync(&self, repo: Option<&Repository>, push: bool) -> Result<()> {
        let Some(repo) = repo else {
            bail!("GIT repo not found");
        };

        let mut index = repo.index().context("cannot get the index file")?;
        let statuses = repo
            .statuses(None)
            .context("Failed to get repository status")?;

        if statuses.is_empty() {
            println!("  {} No changes to sync", "ℹ".blue());
            return Ok(());
        }

        println!("  {} Adding changes to git...", "→".blue());
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .context("Failed to add files to git index")?;
        index.write().context("Failed to write git index")?;

        let tree_id = index.write_tree().context("Failed to write git tree")?;
        let tree = repo.find_tree(tree_id).context("Failed to find git tree")?;

        let signature = repo.signature().context("Failed to get git signature")?;
        let head = repo
            .head()
            .context("Failed to get HEAD reference")?
            .target()
            .context("No HEAD commit found")?;
        let parent_commit = repo
            .find_commit(head)
            .context("Failed to find parent commit")?;

        let commit_id = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Sync dotfiles",
                &tree,
                &[&parent_commit],
            )
            .context("Failed to create commit")?;

        println!(
            "  {} Committed changes: {}",
            "✓".green(),
            commit_id.to_string()[..7].to_string().yellow()
        );

        if push {
            println!("  {} Pushing to remote...", "→".blue());

            let mut remote = repo
                .find_remote("origin")
                .context("Remote 'origin' not found")?;

            let head = repo
                .head()
                .context("Failed to get HEAD reference for push")?;
            let branch_name = head
                .shorthand()
                .ok_or_else(|| anyhow!("Cannot determine current branch"))?;

            remote
                .push(&[&format!("refs/heads/{}", branch_name)], None)
                .context("Failed to push to remote")?;

            println!("  {} Pushed to remote", "✓".green());
        }

        Ok(())
    }
}

fn print_status(file_status: FileStatus, conf_path: &Path) -> Result<()> {
    let (file_pathbuf, status) = (file_status.0, file_status.1);

    let file_name = file_pathbuf
        .strip_prefix(conf_path)
        .context("Failed to strip path prefix")?
        .to_str()
        .map(String::from)
        .context("Failed to convert path to string")?;

    match status {
        Some(Status::INDEX_NEW) => {
            println!("      {} {}{}", "•".yellow(), file_name, " [new]".green())
        }
        Some(Status::INDEX_MODIFIED) => {
            println!(
                "      {} {}{}",
                "•".yellow(),
                file_name,
                " [modified]".yellow()
            )
        }
        Some(Status::INDEX_DELETED) => {
            println!("      {} {}{}", "•".yellow(), file_name, " [deleted]".red())
        }
        Some(Status::WT_NEW) => {
            println!(
                "      {} {}{}",
                "•".yellow(),
                file_name,
                " [untracked]".cyan()
            )
        }
        Some(Status::WT_MODIFIED) => {
            println!(
                "      {} {}{}",
                "•".yellow(),
                file_name,
                " [modified]".yellow()
            )
        }
        Some(Status::WT_DELETED) => {
            println!("      {} {}{}", "•".yellow(), file_name, " [deleted]".red())
        }
        Some(Status::IGNORED) => {
            println!(
                "      {} {}{}",
                "•".yellow(),
                file_name,
                " [ignored]".dimmed()
            )
        }
        Some(_) => {
            println!("      {} {}{}", "•".yellow(), file_name, " [ok]".blue())
        }
        None => {
            println!("      {} {}", "•".yellow(), file_name)
        }
    };

    Ok(())
}

fn move_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst).context("Failed to create destination directory")?;

        for entry in src.read_dir().context("Failed to read source directory")? {
            let entry = entry.context("Failed to read directory entry")?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                move_recursive(&src_path, &dst_path)
                    .context("Failed to move subdirectory recursively")?;
            } else {
                fs::copy(&src_path, &dst_path).context("Failed to copy file")?;
            }
        }
        fs::remove_dir_all(src).context("Failed to remove source directory after move")?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).context("Failed to create parent directories")?;
        }
        fs::copy(src, dst).context("Failed to copy file to destination")?;
        fs::remove_file(src).context("Failed to remove source file after copy")?;
    }

    Ok(())
}

fn move_and_symlink(files: &[PathBuf], to: &Path) -> Result<()> {
    for f in files {
        let dest = to.join(f);
        if dest.exists() {
            println!("{} already existing. Skipping", dest.display());
            continue;
        }

        let f_can_file = f
            .canonicalize()
            .context("Error during file canonicalization")?;

        move_recursive(f, &dest).context("Failed to move file to configuration directory")?;

        #[cfg(unix)]
        unix::fs::symlink(&dest, &f_can_file).context("Error when creating the symlink")?;

        #[cfg(windows)]
        windows::fs::symlink_file(&dest, &f_can_file)
            .context("Failed to create symlink on Windows")?;

        println!(
            "  {} Moved and linked: {} → {}",
            "→".blue(),
            f.display().to_string().yellow(),
            dest.display().to_string().cyan()
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let vault = match env::var("VAULT_DIR") {
        Err(_) => bail!("$VAULT_DIR must be set"),
        Ok(vault) => {
            let vault = PathBuf::from(vault);
            if !vault.is_dir() {
                bail!("$VAULT_DIR is not a directory")
            }
            vault
        }
    };

    let repo = Repository::open(&vault).ok();

    match &args.cmd {
        Commands::Status => args.cmd.do_status(&vault, repo.as_ref())?,
        Commands::Add { conf_name, files } => args.cmd.do_add(&vault, conf_name, files)?,
        Commands::Sync { push } => args.cmd.do_sync(repo.as_ref(), *push)?,
    }

    Ok(())
}
