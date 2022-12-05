use std::{collections::*, path::*};
use vfs::*;

pub fn load(from: PathBuf) -> anyhow::Result<()> {
    let fs = PhysicalFS::new("/");
    let cache = Cache::new(
        &fs,
        &path_to_string(from)?,
        &path_to_string(std::env::current_dir()?)?,
    );

    cache.load()?;

    Ok(())
}

fn path_to_string(path: PathBuf) -> anyhow::Result<String> {
    path.to_str()
        .ok_or(anyhow::anyhow!("path not unicode: {path:?}"))
        .map(|s| s.to_string())
}

pub fn save(to: PathBuf) -> anyhow::Result<()> {
    let fs = PhysicalFS::new("/");
    let cache = Cache::new(
        &fs,
        &path_to_string(to)?,
        &path_to_string(std::env::current_dir()?)?,
    );

    let cache_paths = crate::targets::targets()?
        .into_iter()
        .flat_map(|t| t.cache_paths())
        .map(path_to_string)
        .collect::<Result<HashSet<String>, _>>()?;

    for path in cache_paths {
        cache.save(&path)?;
    }

    Ok(())
}

struct Cache<'f, F: FileSystem> {
    fs: &'f F,
    cache: String,
    pwd: String,
}

impl<'f, F: FileSystem> Cache<'f, F> {
    fn new(fs: &'f F, cache: impl AsRef<str>, pwd: impl AsRef<str>) -> Self {
        Self {
            fs,
            cache: cache.as_ref().to_string(),
            pwd: pwd.as_ref().to_string(),
        }
    }

    fn save(&self, path: &str) -> anyhow::Result<()> {
        if path.starts_with("/") {
            self.copy_into(path, &format!("{}/absolute{path}", self.cache))?;
        } else {
            self.copy_into(
                &format!("{}/{path}", self.pwd),
                &format!("{}/relative/{path}", self.cache),
            )?;
        }
        Ok(())
    }

    fn copy_into(&self, from: &str, to: &str) -> anyhow::Result<()> {
        if !self.fs.exists(from)? {
            return Ok(());
        }

        match self.fs.metadata(from)?.file_type {
            VfsFileType::File => {
                let mut read = self.fs.open_file(from)?;
                let mut write = self.fs.create_file(to)?;

                std::io::copy(&mut read, &mut write)?;
            }

            VfsFileType::Directory => {
                self.create_dir_all(to)?;

                for file in self.fs.read_dir(from)? {
                    self.copy_into(
                        &format!("{from}/{file}"),
                        &format!("{to}/{file}").replace("//", "/"),
                    )?;
                }
            }
        }

        Ok(())
    }

    fn create_dir_all(&self, dir: &str) -> anyhow::Result<()> {
        if self.fs.exists(dir)? {
            return Ok(());
        }

        let (parent, _) = dir.rsplit_once("/").unwrap();
        self.create_dir_all(parent)?;
        self.fs.create_dir(dir)?;

        Ok(())
    }

    fn load(&self) -> anyhow::Result<()> {
        self.copy_into(&format!("{}/absolute", self.cache), "/")?;
        self.copy_into(&format!("{}/relative", self.cache), &self.pwd)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_single_file() {
        let fs = MemoryFS::new();
        fs.create_dir("/src").unwrap();
        write!(fs.create_file("/src/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        fs.remove_file("/src/foo.txt").unwrap();
        cache.load().unwrap();

        let mut foo = String::new();
        fs.open_file("/src/foo.txt")
            .unwrap()
            .read_to_string(&mut foo)
            .unwrap();
        assert_eq!(foo, "foo");
    }

    #[test]
    fn subdirectory() {
        let fs = MemoryFS::new();
        fs.create_dir("/src").unwrap();
        fs.create_dir("/src/subdir").unwrap();
        write!(fs.create_file("/src/subdir/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        fs.remove_file("/src/subdir/foo.txt").unwrap();
        fs.remove_dir("/src/subdir").unwrap();
        cache.load().unwrap();

        let mut foo = String::new();
        fs.open_file("/src/subdir/foo.txt")
            .unwrap()
            .read_to_string(&mut foo)
            .unwrap();
        assert_eq!(foo, "foo");
    }

    #[test]
    fn relative_path() {
        let fs = MemoryFS::new();
        fs.create_dir("/project").unwrap();
        fs.create_dir("/project/src").unwrap();
        write!(fs.create_file("/project/src/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("src").unwrap();
        fs.remove_file("/project/src/foo.txt").unwrap();
        fs.remove_dir("/project/src").unwrap();
        cache.load().unwrap();

        let mut foo = String::new();
        fs.open_file("/project/src/foo.txt")
            .unwrap()
            .read_to_string(&mut foo)
            .unwrap();
        assert_eq!(foo, "foo");
    }
}
