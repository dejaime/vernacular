use std::fmt;
use std::path::PathBuf;

/// Error type for Vernacular parsing and I/O operations.
///
/// This enum covers the two categories of failure that can occur when loading
/// translation files: filesystem I/O errors and format-level parse errors
/// (malformed CSV or RON content).
#[derive(Debug)]
pub enum VernacularError {
    /// A filesystem I/O error (missing file, permission denied, etc.).
    Io(std::io::Error),
    /// A parse error from a translation file (bad CSV structure, invalid RON, etc.).
    Parse(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for VernacularError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VernacularError::Io(e) => write!(f, "I/O error: {}", e),
            VernacularError::Parse(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for VernacularError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VernacularError::Io(e) => Some(e),
            VernacularError::Parse(e) => Some(e.as_ref()),
        }
    }
}

impl From<std::io::Error> for VernacularError {
    fn from(e: std::io::Error) -> Self {
        VernacularError::Io(e)
    }
}

#[cfg(feature = "csv")]
impl From<csv::Error> for VernacularError {
    fn from(e: csv::Error) -> Self {
        VernacularError::Parse(Box::new(e))
    }
}

#[cfg(feature = "ron")]
impl From<ron::error::SpannedError> for VernacularError {
    fn from(e: ron::error::SpannedError) -> Self {
        VernacularError::Parse(Box::new(e))
    }
}

/// An aggregation of multiple errors encountered during a reload.
#[derive(Debug)]
pub struct AggregateError(pub(crate) Vec<VernacularError>);

impl AggregateError {
    /// Returns the number of errors in this aggregate.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if no errors were collected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a slice of the contained errors.
    #[must_use]
    pub fn errors(&self) -> &[VernacularError] {
        &self.0
    }
}

impl fmt::Display for AggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "0 errors occurred")
        } else if self.0.len() == 1 {
            write!(f, "1 error occurred: {}", self.0[0])
        } else {
            writeln!(f, "{} errors occurred:", self.0.len())?;
            for (i, err) in self.0.iter().enumerate() {
                if i == self.0.len() - 1 {
                    write!(f, "  - {}", err)?;
                } else {
                    writeln!(f, "  - {}", err)?;
                }
            }
            Ok(())
        }
    }
}

impl std::error::Error for AggregateError {}

impl IntoIterator for AggregateError {
    type Item = VernacularError;
    type IntoIter = std::vec::IntoIter<VernacularError>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A wrapper for parse errors that associates the error with a specific file path,
/// preserving the original error chain.
///
/// You can downcast via `err.source().and_then(|s| s.downcast_ref::<FileParseError>())`
/// to recover the file path and original error variant.
#[derive(Debug)]
pub struct FileParseError {
    pub(crate) path: PathBuf,
    pub(crate) source: VernacularError,
}

impl FileParseError {
    /// The file path that failed to parse.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl fmt::Display for FileParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Failed to parse file '{}': {}",
            self.path.display(),
            self.source
        )
    }
}

impl std::error::Error for FileParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
