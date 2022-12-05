use std::{collections::*, fmt::Display, path::*, process::*};

mod go;
mod rust;

pub fn targets() -> anyhow::Result<Vec<Box<dyn Target>>> {
    let mut result = Vec::new();

    for entry in ignore::Walk::new("./") {
        let entry = entry?;

        let is_dir = entry.file_type().expect("no stdin/stdout").is_dir();
        if !is_dir {
            continue;
        }
        let path = entry.into_path();

        for factory in TARGET_DISCOVERY {
            result.extend(factory(&path)?);
        }
    }

    Ok(result)
}

#[linkme::distributed_slice]
static TARGET_DISCOVERY: [fn(&Path) -> anyhow::Result<Vec<Box<dyn Target>>>] = [..];

pub trait Target: Display + Send + Sync + 'static {
    fn perform_test(&self) -> anyhow::Result<()>;

    fn cache_paths(&self) -> HashSet<PathBuf> {
        Default::default()
    }
}

trait OutputExt {
    fn success_ok(self) -> Result<StringOutput, StringOutput>;
}

impl OutputExt for Output {
    fn success_ok(self) -> Result<StringOutput, StringOutput> {
        let output = StringOutput {
            stdout: String::from_utf8_lossy(&self.stdout).to_string(),
            stderr: String::from_utf8_lossy(&self.stderr).to_string(),
        };
        if self.status.success() {
            Ok(output)
        } else {
            Err(output)
        }
    }
}

struct StringOutput {
    stdout: String,
    stderr: String,
}
