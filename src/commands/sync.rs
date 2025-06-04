use anyhow::{anyhow, bail, Context, Result};
use git2::{IndexAddOption, Repository};
use owo_colors::OwoColorize;

pub fn execute(repo: Option<&Repository>, push: bool) -> Result<()> {
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
