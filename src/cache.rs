//! Move files between project directory and a cache directory.
//!
//! Moves for better performance by not copying bytes, instead just updating inodes.

use anyhow::Context;
use std::{collections::*, fs::*, path::*};

pub fn load(from: PathBuf) -> anyhow::Result<()> {
    if !from.exists() {
        // The cache isn't guaranteed to exist. The rest of the method will
        // fail if the cache does not exist.
        return Ok(());
    }

    let src_dir = std::env::current_dir()?;

    let relative_dir = from.join("relative");
    let relative = walkdir::WalkDir::new(&relative_dir)
        .into_iter()
        .flat_map(|r| r.ok())
        .filter_map(into_file_path);

    for path in relative {
        let to = src_dir.join(path.strip_prefix(&relative_dir).unwrap());
        move_file(path, to)?;
    }

    let absolute_dir = from.join("absolute");
    let absolute = walkdir::WalkDir::new(&absolute_dir)
        .into_iter()
        .flat_map(|r| r.ok())
        .filter_map(into_file_path);

    for path in absolute {
        let to = PathBuf::from("/").join(&path.strip_prefix(&absolute_dir).unwrap());
        move_file(path, to)?;
    }

    Ok(())
}

fn into_file_path(entry: walkdir::DirEntry) -> Option<PathBuf> {
    if entry.file_type().is_file() {
        Some(entry.into_path())
    } else {
        None
    }
}

pub fn save(to: PathBuf) -> anyhow::Result<()> {
    let cache_paths = crate::targets::targets()?
        .into_iter()
        .flat_map(|t| t.cache_paths())
        .collect::<HashSet<PathBuf>>()
        .into_iter()
        .flat_map(|p| walkdir::WalkDir::new(p))
        .filter_map(|r| into_file_path(r.ok()?));

    for path in cache_paths {
        if path.is_relative() {
            move_file(&path, to.join("relative").join(&path))?;
        } else {
            move_file(
                &path,
                to.join("absolute").join(&path.strip_prefix("/").unwrap()),
            )?;
        }
    }

    Ok(())
}

fn move_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> anyhow::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());

    create_dir_all(to.parent().unwrap()).context("Creating parent directory")?;
    rename(from, to).context("Renaming file")?;

    Ok(())
}
