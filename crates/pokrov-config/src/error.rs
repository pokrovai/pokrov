use std::{error::Error, fmt, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
}

impl ValidationIssue {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: serde_yaml::Error },
    Validation { path: PathBuf, issues: Vec<ValidationIssue> },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read config '{}': {}", path.display(), source)
            }
            Self::Parse { path, source } => {
                write!(f, "failed to parse config '{}': {}", path.display(), source)
            }
            Self::Validation { path, issues } => {
                write!(
                    f,
                    "config validation failed for '{}': {}",
                    path.display(),
                    issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.field, issue.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Validation { .. } => None,
        }
    }
}
