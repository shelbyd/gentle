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
        if !self.fs.exists(from).context("Checking file existence")? {
            return Ok(());
        }

        let metadata = self.fs.metadata(from).context("Getting file metadata")?;
        match metadata.file_type {
            VfsFileType::File if metadata.len >= DEDUPLICATE_LARGER_THAN => {
                let mut hasher = blake3::Hasher::new();

                let mut src_file = self.fs.open_file(from)?;
                std::io::copy(&mut src_file, &mut hasher)?;
                let hash = hasher.finalize().to_hex();

                self.create_dir_all(&format!("{}/large_files", self.cache))?;
                let contents_path = format!("{}/large_files/{hash}", self.cache);
                if !self.fs.exists(&contents_path)? {
                    let mut contents_dest = self
                        .fs
                        .create_file(&format!("{}/large_files/{hash}", self.cache))?;
                    src_file.rewind()?;
                    std::io::copy(&mut src_file, &mut contents_dest)?;
                }

                let mut write = self.fs.create_file(to)?;
                write.write_all(HASHED_FILE_PREFIX)?;
                write.write_all(hash.as_ref().as_bytes())?;
            }

            VfsFileType::File => {
                let mut read = self
                    .fs
                    .open_file(from)
                    .context(format!("Opening {from:?}"))?;
                let mut write = self
                    .fs
                    .create_file(to)
                    .context(format!("Creating {to:?}"))?;

                let mut contents = Vec::with_capacity(metadata.len as usize);
                read.read_to_end(&mut contents)?;

                let is_hashed = contents.starts_with(HASHED_FILE_PREFIX)
                    && contents.len() == HASHED_FILE_PREFIX.len() + 64;
                if is_hashed {
                    let hash = blake3::Hash::from_hex(&contents[HASHED_FILE_PREFIX.len()..])?;

                    let mut contents = self
                        .fs
                        .open_file(&format!("{}/large_files/{hash}", self.cache))?;
                    std::io::copy(&mut contents, &mut write)?;
                } else {
                    write.write_all(&contents)?;
                }
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

    #[test]
    fn large_duplicate_files_are_only_stored_once() {
        let fs = MemoryFS::new();
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
        fs.remove_file("/src/foo0.txt").unwrap();
        fs.remove_file("/src/foo1.txt").unwrap();

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
        let fs = MemoryFS::new();
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
        fs.remove_file("/src/foo0.txt").unwrap();
        fs.remove_file("/src/foo1.txt").unwrap();
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
}
