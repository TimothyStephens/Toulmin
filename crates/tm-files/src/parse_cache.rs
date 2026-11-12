//! The parse cache (item 48): one Parquet file per (file fingerprint,
//! spec fingerprint) under the cache directory, written with the `parquet`
//! Arrow writer and read back with its Arrow reader, whose stored schema
//! restores item 47's types.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tm_formats::id::EventId;

use crate::{Error, Result};

/// The cache directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCache {
    dir: PathBuf,
}

fn hex(id: &EventId) -> &str {
    id.as_str().trim_start_matches("sha256:")
}

impl ParseCache {
    /// A cache rooted at `dir` (created on first write).
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> ParseCache {
        ParseCache { dir: dir.into() }
    }

    /// The root.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `<cache>/<file fingerprint hex>/<spec fingerprint hex>.parquet`.
    #[must_use]
    pub fn path(&self, file_fingerprint: &EventId, spec_fingerprint: &EventId) -> PathBuf {
        self.dir
            .join(hex(file_fingerprint))
            .join(format!("{}.parquet", hex(spec_fingerprint)))
    }

    /// Write `batch` for the pair, through a temporary name beside it.
    ///
    /// # Errors
    /// `Error::Io`, `Error::Parquet`.
    pub fn store(
        &self,
        file_fingerprint: &EventId,
        spec_fingerprint: &EventId,
        batch: &RecordBatch,
    ) -> Result<PathBuf> {
        let path = self.path(file_fingerprint, spec_fingerprint);
        let parent = path
            .parent()
            .ok_or_else(|| Error::io(&path, std::io::Error::other("no parent")))?;
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        let tmp = parent.join(format!(
            ".{}.tmp",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("cache")
        ));
        let file = File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        let mut writer = ArrowWriter::try_new(file, batch.schema(), None)?;
        writer.write(batch)?;
        let file = writer.into_inner()?;
        file.sync_all().map_err(|e| Error::io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))?;
        Ok(path)
    }

    /// Read the pair back, or `None` when it was never written.
    ///
    /// # Errors
    /// `Error::Io`, `Error::Parquet`, `Error::Arrow`.
    pub fn load(
        &self,
        file_fingerprint: &EventId,
        spec_fingerprint: &EventId,
    ) -> Result<Option<RecordBatch>> {
        let path = self.path(file_fingerprint, spec_fingerprint);
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(&path, e)),
        };
        Ok(Some(read_parquet(file)?))
    }
}

/// Read a whole Parquet file as one batch (empty batch with the schema when
/// it has no rows).
///
/// # Errors
/// `Error::Parquet`, `Error::Arrow`.
pub(crate) fn read_parquet(file: File) -> Result<RecordBatch> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let schema = builder.schema().clone();
    let reader = builder.build()?;
    let mut batches = Vec::new();
    for b in reader {
        batches.push(b?);
    }
    if batches.is_empty() {
        return Ok(RecordBatch::new_empty(Arc::clone(&schema)));
    }
    Ok(arrow::compute::concat_batches(&schema, &batches)?)
}
