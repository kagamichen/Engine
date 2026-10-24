use std::io;
use std::path::Path;

use async_trait::async_trait;
use uc_application::facade::{ExportFileSource, ExportFileSourcePort};

pub struct FsExportFileReader;

#[async_trait]
impl ExportFileSourcePort for FsExportFileReader {
    async fn open(&self, path: &Path) -> io::Result<ExportFileSource> {
        if !tokio::fs::metadata(path).await?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "export source is not a regular file",
            ));
        }
        let file = tokio::fs::File::open(path).await?;
        let metadata = file.metadata().await?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "export source is not a regular file",
            ));
        }
        Ok(ExportFileSource {
            length: metadata.len(),
            reader: Box::new(file),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn reads_regular_and_empty_files_and_rejects_missing_or_directory() {
        let root = tempfile::tempdir().unwrap();
        for content in [b"payload".as_slice(), b""] {
            let path = root.path().join("file");
            std::fs::write(&path, content).unwrap();
            let mut source = FsExportFileReader.open(&path).await.unwrap();
            assert_eq!(source.length, content.len() as u64);
            let mut actual = Vec::new();
            source.reader.read_to_end(&mut actual).await.unwrap();
            assert_eq!(actual, content);
        }
        assert!(
            matches!(FsExportFileReader.open(root.path()).await, Err(error) if error.kind() == io::ErrorKind::InvalidInput)
        );
        assert!(
            matches!(FsExportFileReader.open(&root.path().join("absent")).await, Err(error) if error.kind() == io::ErrorKind::NotFound)
        );
    }
}
