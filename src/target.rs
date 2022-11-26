use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub package: String,
    pub task: String,
}

impl FromStr for Target {
    type Err = TargetParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(':');

        let package = split.next().unwrap();
        if package.is_empty() {
            return Err(TargetParseError::MissingPackage);
        }
        let package = package
            .strip_prefix("//")
            .ok_or(TargetParseError::PackageMustBeAbsolute)?
            .to_string();

        Ok(Target {
            package,
            task: split
                .next()
                .ok_or(TargetParseError::MissingTask)?
                .to_string(),
        })
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "//{}:{}", self.package, self.task)
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum TargetParseError {
    #[error("missing task")]
    MissingTask,

    #[error("missing package")]
    MissingPackage,

    #[error("package must be absolute")]
    PackageMustBeAbsolute,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_qualified() {
        assert_eq!(
            "//foo/bar:baz".parse(),
            Ok(Target {
                package: "foo/bar".to_string(),
                task: "baz".to_string(),
            })
        );
    }

    #[test]
    fn missing_task() {
        assert_eq!(
            "//foo/bar".parse::<Target>(),
            Err(TargetParseError::MissingTask),
        );
    }

    #[test]
    fn missing_package() {
        assert_eq!(
            ":baz".parse::<Target>(),
            Err(TargetParseError::MissingPackage),
        );
    }

    #[test]
    fn relative_target() {
        assert_eq!(
            "foo/bar:baz".parse::<Target>(),
            Err(TargetParseError::PackageMustBeAbsolute),
        );
    }
}
