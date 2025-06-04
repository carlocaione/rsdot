use anyhow::{Context, Result};
use git2::{Repository, Status};
use owo_colors::OwoColorize;
use std::{fs, path::Path};
use walkdir::WalkDir;

pub struct FileStatus(pub(crate) PathBuf, pub(crate) Option<Status>);

use std::path::PathBuf;

pub fn execute(vault: &Path, repo: Option<&Repository>) -> Result<()> {
    println!();
    println!(
        "  {} Vault location: {}",
        "→".blue(),
        vault.display().cyan()
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
        .with_context(|| format!("Failed to read {}", vault.display()))?
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
                print_status(file_status, &conf_path).with_context(|| {
                    format!("Failed to print file status for {}", conf_path.display())
                })?;
            }
        }

        println!();
    }

    Ok(())
}

fn print_status(file_status: FileStatus, conf_path: &Path) -> Result<()> {
    let (file_pathbuf, status) = (file_status.0, file_status.1);

    let file_name = file_pathbuf
        .strip_prefix(conf_path)
        .with_context(|| format!("Failed to strip path prefix from {}", conf_path.display()))?
        .to_str()
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
