use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentError {
    Io { path: PathBuf, message: String },
    Parse { source: String, message: String },
    Validation { source: String, message: String },
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    f,
                    "failed to read content file {}: {message}",
                    path.display()
                )
            }
            Self::Parse { source, message } => write!(f, "failed to parse {source}: {message}"),
            Self::Validation { source, message } => {
                write!(f, "invalid content in {source}: {message}")
            }
        }
    }
}

impl std::error::Error for ContentError {}
