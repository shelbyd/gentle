use anyhow::Context;
use std::{collections::*, path::*};
use vfs::*;

const DEDUPLICATE_LARGER_THAN: u64 = 1024;
const HASHED_FILE_PREFIX: &[u8] = b"GENTLE HASHED";

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

    pub(crate) fn save(&self, path: &str) -> anyhow::Result<()> {
        self.create_dir_all(&format!("{}/large_files", self.cache))?;

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
        if !self.fs.exists(from).context("Checking file existence")? {
            return Ok(());
        }

        let metadata = self.fs.metadata(from).context("Getting file metadata")?;
        match metadata.file_type {
            VfsFileType::Directory => {
                self.create_dir_all(to)?;

                for file in self.fs.read_dir(from)? {
                    self.copy_into(
                        &format!("{from}/{file}"),
                        &format!("{to}/{file}").replace("//", "/"),
                    )?;
                }
                return Ok(());
            }

            VfsFileType::File => {}
        }

        let mut from_file = self.fs.open_file(from).context("Opening {from:?}")?;

        let copy_from = {
            let mut result = from.to_string();
            if metadata.len as usize == HASHED_FILE_PREFIX.len() + 64 {
                let mut contents = Vec::with_capacity(metadata.len as usize);
                from_file.read_to_end(&mut contents)?;

                if contents.starts_with(HASHED_FILE_PREFIX) {
                    let hash = blake3::Hash::from_hex(&contents[HASHED_FILE_PREFIX.len()..])?;
                    result = format!("{}/large_files/{hash}", self.cache);
                }
            }
            result
        };

        let copy_to = if metadata.len < DEDUPLICATE_LARGER_THAN {
            to.to_string()
        } else {
            let mut hasher = blake3::Hasher::new();
            std::io::copy(&mut from_file, &mut hasher)?;
            let hash = hasher.finalize().to_hex();

            let mut write = self.fs.create_file(to)?;
            write.write_all(HASHED_FILE_PREFIX)?;
            write.write_all(hash.as_ref().as_bytes())?;

            format!("{}/large_files/{hash}", self.cache)
        };

        self.fs.copy_file(&copy_from, &copy_to)?;

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

    pub(crate) fn load(&self) -> anyhow::Result<()> {
        self.copy_into(&format!("{}/absolute", self.cache), "/")
            .context("Loading absolute paths")?;
        self.copy_into(&format!("{}/relative", self.cache), &self.pwd)
            .context("Loading relative paths")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn save_load_single_file() {
        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/src").unwrap();
        write!(fs.create_file("/src/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        let _ = fs.remove_file("/src/foo.txt");
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
        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/src").unwrap();
        fs.create_dir("/src/subdir").unwrap();
        write!(fs.create_file("/src/subdir/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        let _ = fs.remove_file("/src/subdir/foo.txt");
        let _ = fs.remove_dir("/src/subdir");
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
        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/project").unwrap();
        fs.create_dir("/project/src").unwrap();
        write!(fs.create_file("/project/src/foo.txt").unwrap(), "foo").unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("src").unwrap();
        let _ = fs.remove_file("/project/src/foo.txt");
        let _ = fs.remove_dir("/project/src");
        cache.load().unwrap();

        let mut foo = String::new();
        fs.open_file("/project/src/foo.txt")
            .unwrap()
            .read_to_string(&mut foo)
            .unwrap();
        assert_eq!(foo, "foo");
    }

    #[test]
    fn large_duplicate_files_are_only_stored_once() {
        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/src").unwrap();
        fs.create_file("/src/foo0.txt")
            .unwrap()
            .write_all(&[0; 1024])
            .unwrap();
        fs.create_file("/src/foo1.txt")
            .unwrap()
            .write_all(&[0; 1024])
            .unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        let _ = fs.remove_file("/src/foo0.txt");
        let _ = fs.remove_file("/src/foo1.txt");

        let path = VfsPath::from(fs);
        let total_file_size = path
            .walk_dir()
            .unwrap()
            .map(|r| Ok::<u64, VfsError>(r?.metadata()?.len))
            .sum::<Result<u64, _>>()
            .unwrap();

        assert_eq!(
            total_file_size,
            1024 + (64 + HASHED_FILE_PREFIX.len() as u64) * 2
        );
    }

    #[test]
    fn recovers_large_files() {
        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/src").unwrap();
        fs.create_file("/src/foo0.txt")
            .unwrap()
            .write_all(&[0; 1024])
            .unwrap();
        fs.create_file("/src/foo1.txt")
            .unwrap()
            .write_all(&[0; 1024])
            .unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        let _ = fs.remove_file("/src/foo0.txt");
        let _ = fs.remove_file("/src/foo1.txt");
        cache.load().unwrap();

        let mut vec = Vec::new();
        fs.open_file("/src/foo0.txt")
            .unwrap()
            .read_to_end(&mut vec)
            .unwrap();
        assert_eq!(vec, vec![0; 1024]);

        let mut vec = Vec::new();
        fs.open_file("/src/foo1.txt")
            .unwrap()
            .read_to_end(&mut vec)
            .unwrap();
        assert_eq!(vec, vec![0; 1024]);
    }

    #[test]
    fn copies_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let fs = PhysicalFS::new(dir.path());

        fs.create_dir("/src").unwrap();
        write!(fs.create_file("/src/foo.exe").unwrap(), "foo").unwrap();

        let file_path = dir.path().join("src/foo.exe");
        set_permissions(&file_path, Permissions::from_mode(0o755)).unwrap();

        let cache = Cache::new(&fs, "/cache", "/project");

        cache.save("/src").unwrap();
        let _ = fs.remove_file("/src/foo.exe");
        cache.load().unwrap();

        let metadata = metadata(&file_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
    }
}
