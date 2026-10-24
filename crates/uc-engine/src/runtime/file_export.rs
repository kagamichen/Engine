use std::io::{self, Seek, SeekFrom, Write};

use tokio_util::sync::CancellationToken;

use super::host_operations::map_export_resource_error;
use super::{operation_unavailable_error, ProductionRuntime};
use crate::{
    EngineError, ExportEntryInput, HostCapabilityError, HostCapabilityErrorCategory,
    HostFileAccess, HostFileHandle, OperationResult,
};
use uc_application::facade::FileExportDestination;
use uc_infra_local::fs::export_file_reader::FsExportFileReader;

const MAX_HOST_WRITE_BYTES: usize = 64 * 1024;

struct HostZipDestination<'a> {
    files: &'a dyn HostFileAccess,
    handle: HostFileHandle,
    cancellation: &'a CancellationToken,
    session_cancellation: &'a CancellationToken,
    position: u64,
    high_water: u64,
}

impl<'a> HostZipDestination<'a> {
    fn new(
        files: &'a dyn HostFileAccess,
        handle: HostFileHandle,
        cancellation: &'a CancellationToken,
        session_cancellation: &'a CancellationToken,
    ) -> Self {
        Self {
            files,
            handle,
            cancellation,
            session_cancellation,
            position: 0,
            high_water: 0,
        }
    }

    fn host_error(error: HostCapabilityError) -> io::Error {
        let kind = match error.category() {
            HostCapabilityErrorCategory::InvalidHandle => io::ErrorKind::InvalidInput,
            HostCapabilityErrorCategory::PermissionDenied => io::ErrorKind::PermissionDenied,
            HostCapabilityErrorCategory::Unavailable | HostCapabilityErrorCategory::Io => {
                io::ErrorKind::Other
            }
        };
        io::Error::new(kind, error)
    }
}

impl Write for HostZipDestination<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.ensure_open()?;
        let bytes = &bytes[..bytes.len().min(MAX_HOST_WRITE_BYTES)];
        if bytes.is_empty() {
            return Ok(0);
        }
        let next_position = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "archive offset overflow")
            })?;
        self.files
            .write_chunk(&self.handle, self.position, bytes)
            .map_err(Self::host_error)?;
        self.position = next_position;
        self.high_water = self.high_water.max(self.position);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.ensure_open()?;
        Ok(())
    }
}

impl Seek for HostZipDestination<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.ensure_open()?;
        let base = match from {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.high_water) + i128::from(offset),
        };
        if base < 0 || base > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "archive seek offset out of range",
            ));
        }
        self.position = base as u64;
        Ok(self.position)
    }
}

impl FileExportDestination for HostZipDestination<'_> {
    fn ensure_open(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() || self.session_cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "export operation cancelled",
            ));
        }
        Ok(())
    }

    fn complete(&mut self) -> io::Result<()> {
        self.ensure_open()?;
        self.files
            .finish_write(&self.handle)
            .map_err(Self::host_error)
    }
}

impl ProductionRuntime {
    pub(super) async fn execute_export_entry_files(
        &self,
        input: ExportEntryInput,
        cancellation: &CancellationToken,
        session_cancellation: &CancellationToken,
    ) -> Result<OperationResult, EngineError> {
        let facade = self.current_facade().await?;
        let source_reader = FsExportFileReader;
        let mut destination = HostZipDestination::new(
            self.files.as_ref(),
            input.destination,
            cancellation,
            session_cancellation,
        );
        facade
            .export_entry_files(&input.entry_id, &source_reader, &mut destination)
            .await
            .map(|()| OperationResult::EntryExported)
            .map_err(|error| {
                if cancellation.is_cancelled() || session_cancellation.is_cancelled() {
                    operation_unavailable_error()
                } else {
                    map_export_resource_error(error)
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::map_export_resource_error;
    use super::HostZipDestination;
    use crate::{EngineErrorCategory, HostCapabilityErrorCategory};
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;
    use uc_application::facade::FileExportDestination;
    use uc_application::facade::ResourceFacadeError;

    use crate::{HostCapabilityError, HostFileAccess, HostFileHandle, HostFileMetadata};

    struct RecordingFiles(Mutex<Vec<(u64, Vec<u8>)>>);

    impl HostFileAccess for RecordingFiles {
        fn metadata(
            &self,
            _handle: &HostFileHandle,
        ) -> Result<HostFileMetadata, HostCapabilityError> {
            unreachable!()
        }

        fn read_chunk(
            &self,
            _handle: &HostFileHandle,
            _offset: u64,
            _max_bytes: u32,
        ) -> Result<Vec<u8>, HostCapabilityError> {
            unreachable!()
        }

        fn write_chunk(
            &self,
            _handle: &HostFileHandle,
            offset: u64,
            bytes: &[u8],
        ) -> Result<(), HostCapabilityError> {
            self.0.lock().unwrap().push((offset, bytes.to_vec()));
            Ok(())
        }

        fn finish_write(&self, _handle: &HostFileHandle) -> Result<(), HostCapabilityError> {
            Ok(())
        }
    }

    #[test]
    fn destination_supports_zip_header_rewrites() {
        let files = RecordingFiles(Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();
        let session = CancellationToken::new();
        let mut destination = HostZipDestination::new(
            &files,
            HostFileHandle::new("destination"),
            &cancel,
            &session,
        );
        destination.write_all(b"header").unwrap();
        destination.seek(SeekFrom::Start(2)).unwrap();
        destination.write_all(b"XX").unwrap();
        assert_eq!(
            files.0.lock().unwrap().as_slice(),
            &[(0, b"header".to_vec()), (2, b"XX".to_vec())]
        );
    }

    #[test]
    fn destination_rejects_writes_after_cancellation() {
        let files = RecordingFiles(Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();
        let session = CancellationToken::new();
        cancel.cancel();
        let mut destination = HostZipDestination::new(
            &files,
            HostFileHandle::new("destination"),
            &cancel,
            &session,
        );
        assert_eq!(
            destination.write_all(b"x").unwrap_err().kind(),
            std::io::ErrorKind::ConnectionAborted
        );
        assert!(destination.seek(SeekFrom::Start(0)).is_err());
        assert!(destination.complete().is_err());
        assert!(files.0.lock().unwrap().is_empty());
    }
    #[test]
    fn wrapped_host_failures_retain_public_categories() {
        for (category, code, expected) in [
            (
                HostCapabilityErrorCategory::InvalidHandle,
                1272,
                EngineErrorCategory::InvalidInput,
            ),
            (
                HostCapabilityErrorCategory::PermissionDenied,
                1273,
                EngineErrorCategory::Unauthorized,
            ),
            (
                HostCapabilityErrorCategory::Io,
                1274,
                EngineErrorCategory::Unavailable,
            ),
        ] {
            let error = HostZipDestination::host_error(HostCapabilityError::new(
                category,
                "private detail",
            ));
            assert!(error
                .get_ref()
                .unwrap()
                .downcast_ref::<HostCapabilityError>()
                .is_some());
            let error =
                ResourceFacadeError::Internal(anyhow::Error::new(error).context("write archive"));
            let mapped = map_export_resource_error(error);
            assert_eq!(mapped.code(), code);
            assert_eq!(mapped.category(), expected);
        }
    }

    #[test]
    fn destination_bounds_writes_and_tracks_end_after_rewrites() {
        let files = RecordingFiles(Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();
        let session = CancellationToken::new();
        let mut destination =
            HostZipDestination::new(&files, HostFileHandle::new("out"), &cancel, &session);
        let payload = vec![7; 150_000];
        destination.write_all(&payload).unwrap();
        assert!(files
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|(_, bytes)| bytes.len() <= 65536));
        destination.seek(SeekFrom::Start(0)).unwrap();
        destination.write_all(b"header").unwrap();
        assert_eq!(destination.seek(SeekFrom::End(0)).unwrap(), 150_000);
        assert!(destination.seek(SeekFrom::Current(-150_001)).is_err());
        destination.seek(SeekFrom::Start(u64::MAX)).unwrap();
        let before = files.0.lock().unwrap().len();
        assert!(destination.write_all(b"x").is_err());
        assert_eq!(files.0.lock().unwrap().len(), before);
    }
}
