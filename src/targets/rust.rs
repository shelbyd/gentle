use super::*;

#[linkme::distributed_slice(TARGET_DISCOVERY)]
fn discover(path: &Path) -> anyhow::Result<Vec<Box<dyn Target>>> {
    if path.join("Cargo.toml").try_exists()? {
        Ok(vec![(Box::new(RustCargoTarget::new(&path)))])
    } else {
        Ok(Vec::new())
    }
}

pub struct RustCargoTarget {
    path: PathBuf,
}

impl RustCargoTarget {
    fn new(path: &Path) -> Self {
        Self { path: path.into() }
    }
}

impl Display for RustCargoTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO(shelbyd): De-duplicate formatting of target addresses.
        let package = self.path.display().to_string().replacen("./", "", 1);
        write!(f, "//{package}:rust_crate")
    }
}

impl Target for RustCargoTarget {
    fn perform_test(&self) -> anyhow::Result<()> {
        Command::new("cargo")
            .args(&[
                "test",
                "--manifest-path",
                &self.path.join("Cargo.toml").to_string_lossy(),
                "--jobs=1",
                "--color=always",
            ])
            .output()?
            .success_ok()
            .map(|_| ())
            .map_err(|out| anyhow::anyhow!(format!("{}\n{}", out.stderr, out.stdout)))
    }

    fn cache_paths(&self) -> HashSet<PathBuf> {
        [self.path.join("target")].into_iter().collect()
    }
}
