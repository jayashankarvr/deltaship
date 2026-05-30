use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::DeltashipError;
use crate::version::Version;

/// Differential patching algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffAlgorithm {
    Bsdiff,
    Courgette,
    Xdelta3,
}

impl DiffAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffAlgorithm::Bsdiff => "bsdiff",
            DiffAlgorithm::Courgette => "courgette",
            DiffAlgorithm::Xdelta3 => "xdelta3",
        }
    }
}

impl fmt::Display for DiffAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DiffAlgorithm {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bsdiff" => Ok(DiffAlgorithm::Bsdiff),
            "courgette" => Ok(DiffAlgorithm::Courgette),
            "xdelta3" => Ok(DiffAlgorithm::Xdelta3),
            _ => Err(DeltashipError::InvalidDiffAlgorithm(s.to_string())),
        }
    }
}

/// Compression format for diff files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompressionFormat {
    #[default]
    None,
    Zstd,
    Gzip,
}

impl CompressionFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompressionFormat::None => "none",
            CompressionFormat::Zstd => "zstd",
            CompressionFormat::Gzip => "gzip",
        }
    }
}

impl fmt::Display for CompressionFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CompressionFormat {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(CompressionFormat::None),
            "zstd" => Ok(CompressionFormat::Zstd),
            "gzip" | "gz" => Ok(CompressionFormat::Gzip),
            _ => Err(DeltashipError::InvalidCompressionFormat(s.to_string())),
        }
    }
}

/// Unique identifier for a diff
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiffId(Uuid);

impl DiffId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for DiffId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DiffId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DiffId {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| DeltashipError::InvalidDiffId(e.to_string()))
    }
}

/// Metadata for a differential patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffMetadata {
    pub id: DiffId,
    pub from_version: Version,
    pub to_version: Version,
    pub algorithm: DiffAlgorithm,
    pub size_bytes: u64,
    pub hash: String,
    pub compression: CompressionFormat,
}

impl DiffMetadata {
    #[must_use]
    pub fn new(
        from_version: Version,
        to_version: Version,
        algorithm: DiffAlgorithm,
        size_bytes: u64,
        hash: String,
    ) -> Self {
        Self {
            id: DiffId::new(),
            from_version,
            to_version,
            algorithm,
            size_bytes,
            hash,
            compression: CompressionFormat::default(),
        }
    }

    #[must_use]
    pub fn with_compression(mut self, compression: CompressionFormat) -> Self {
        self.compression = compression;
        self
    }
}
