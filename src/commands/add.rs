use anyhow::{Context, Result};
use inquire::Confirm;
use owo_colors::OwoColorize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix;

#[cfg(windows)]
use std::os::windows;

pub fn execute(vault: &Path, conf_name: &str, files: &Option<Vec<PathBuf>>) -> Result<()> {
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

        fs::create_dir_all(&conf_path)
            .with_context(|| format!("Cannot create {}", conf_path.display()))?;

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

    move_and_symlink(files, &conf_path).with_context(|| {
        format!(
            "Failed to move and symlink files for {}",
            conf_path.display()
        )
    })?;

    Ok(())
}

fn move_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst).with_context(|| format!("Failed to create {}", dst.display()))?;

        for entry in src
            .read_dir()
            .with_context(|| format!("Failed to read {}", src.display()))?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                move_recursive(&src_path, &dst_path).with_context(|| {
                    format!(
                        "Failed to move {} into {}",
                        src_path.display(),
                        dst_path.display()
                    )
                })?;
            } else {
                fs::copy(&src_path, &dst_path).with_context(|| {
                    format!(
                        "Failed to copy {} into {}",
                        src_path.display(),
                        dst_path.display()
                    )
                })?;
            }
        }
        fs::remove_dir_all(src).with_context(|| format!("Failed to remove {}", src.display()))?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        fs::copy(src, dst)
            .with_context(|| format!("Failed to copy {} to {}", src.display(), dst.display()))?;
        fs::remove_file(src).with_context(|| format!("Failed to remove {}", src.display()))?;
    }

    Ok(())
}

fn move_and_symlink(files: &[PathBuf], to: &Path) -> Result<()> {
    for f in files {
        let dest = to.join(f);
        if dest.exists() {
            println!(
                "  {} {} already existing. Skipping",
                "⚠".yellow(),
                dest.display().cyan()
            );
            continue;
        }

        let f_can_file = f
            .canonicalize()
            .context("Error during file canonicalization")?;

        move_recursive(f, &dest)
            .with_context(|| format!("Failed to move {} to {}", f.display(), dest.display()))?;

        #[cfg(unix)]
        unix::fs::symlink(&dest, &f_can_file).with_context(|| {
            format!(
                "Error when creating the symlink form {} to {}",
                dest.display(),
                f_can_file.display()
            )
        })?;

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
