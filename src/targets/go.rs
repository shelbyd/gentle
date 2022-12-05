use super::*;

#[linkme::distributed_slice(TARGET_DISCOVERY)]
fn discover(path: &Path) -> anyhow::Result<Vec<Box<dyn Target>>> {
    if path.join("go.mod").try_exists()? {
        Ok(vec![(Box::new(GoModTarget::new(&path)))])
    } else {
        Ok(Vec::new())
    }
}

pub struct GoModTarget {
    path: PathBuf,
}

impl GoModTarget {
    pub fn new(path: &Path) -> Self {
        Self { path: path.into() }
    }

    fn cache_dir(&self) -> PathBuf {
        std::env::var("GOCACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                Path::new(&std::env::var("HOME").unwrap_or(String::from("/")))
                    .join(".cache/go-build")
            })
            .into()
    }
}

impl Display for GoModTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let package = self.path.display().to_string().replacen("./", "", 1);
        write!(f, "//{package}:go_mod")
    }
}

impl Target for GoModTarget {
    fn perform_test(&self) -> anyhow::Result<()> {
        let out = Command::new("go")
            .args(&["test"])
            .env("GOCACHE", self.cache_dir())
            .current_dir(&self.path)
            .output()?;

        out.success_ok()
            .map(|_| ())
            .map_err(|out| anyhow::anyhow!(out.stderr))
    }

    fn cache_paths(&self) -> HashSet<PathBuf> {
        [self.cache_dir()].into_iter().collect()
    }
}
