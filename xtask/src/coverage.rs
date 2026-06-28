use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn run() -> Result<()> {
    let root = find_workspace_root()?;
    let status = Command::new("python3")
        .arg(root.join("scripts/generate_requirement_coverage.py"))
        .current_dir(&root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .context("failed to execute requirement coverage script")?;

    if !status.success() {
        anyhow::bail!("requirement coverage script reported uncovered or invalid requirements");
    }

    println!("requirement coverage report is up-to-date");
    Ok(())
}

fn find_workspace_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!("could not find workspace root containing Cargo.toml");
        }
    }
}
