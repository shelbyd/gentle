use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct TargetAddress {
    pub package: String,
    pub identifier: String,
}

impl FromStr for TargetAddress {
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

        Ok(TargetAddress {
            package,
            identifier: split
                .next()
                .ok_or(TargetParseError::MissingTask)?
                .to_string(),
        })
    }
}

impl std::fmt::Display for TargetAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "//{}:{}", self.package, self.identifier)
    }
}

#[derive(Debug)]
pub struct TargetMatcher {
    package: String,
    identifier: Option<String>,
}

impl TargetMatcher {
    fn matches(&self, target: &TargetAddress) -> bool {
        if self.package != "..." && self.package != target.package {
            return false;
        }

        if let Some(ident) = self.identifier.as_ref() {
            if ident != &target.identifier {
                return false;
            }
        }

        true
    }
}

impl FromStr for TargetMatcher {
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

        Ok(TargetMatcher {
            package,
            identifier: split.next().map(ToString::to_string),
        })
    }
}

pub trait Matches {
    fn matches(&self, target: &TargetAddress) -> bool;
}

impl<'t, T> Matches for T
where
    T: AsRef<[TargetMatcher]> + ?Sized,
{
    fn matches(&self, target: &TargetAddress) -> bool {
        self.as_ref().iter().any(|m| m.matches(target))
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

    mod address {
        use super::*;

        #[test]
        fn fully_qualified() {
            assert_eq!(
                "//foo/bar:baz".parse(),
                Ok(TargetAddress {
                    package: "foo/bar".to_string(),
                    identifier: "baz".to_string(),
                })
            );
        }

        #[test]
        fn missing_task() {
            assert_eq!(
                "//foo/bar".parse::<TargetAddress>(),
                Err(TargetParseError::MissingTask),
            );
        }

        #[test]
        fn missing_package() {
            assert_eq!(
                ":baz".parse::<TargetAddress>(),
                Err(TargetParseError::MissingPackage),
            );
        }

        #[test]
        fn relative_target() {
            assert_eq!(
                "foo/bar:baz".parse::<TargetAddress>(),
                Err(TargetParseError::PackageMustBeAbsolute),
            );
        }
    }

    mod matcher {
        use super::*;

        #[test]
        fn exact_target() {
            let matcher: TargetMatcher = "//foo/bar:baz".parse().unwrap();
            let target: TargetAddress = "//foo/bar:baz".parse().unwrap();
            assert!(&[matcher][..].matches(&target));
        }

        #[test]
        fn different_target() {
            let matcher: TargetMatcher = "//foo/bar:baz".parse().unwrap();
            let target: TargetAddress = "//foo/bar:qux".parse().unwrap();
            assert!(!&[matcher][..].matches(&target));
        }

        #[test]
        fn different_package() {
            let matcher: TargetMatcher = "//foo/bar:baz".parse().unwrap();
            let target: TargetAddress = "//foo/qux:baz".parse().unwrap();
            assert!(!&[matcher][..].matches(&target));
        }

        #[test]
        fn root_matcher() {
            let matcher: TargetMatcher = "//...".parse().unwrap();
            let target: TargetAddress = "//foo/qux:baz".parse().unwrap();
            assert!(&[matcher][..].matches(&target));
        }

        // TODO(shelbyd): More powerful matching.
    }
}
