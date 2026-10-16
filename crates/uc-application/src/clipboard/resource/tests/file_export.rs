use super::{seed_entry_with_file_rep, test_deps, TestDeps};
use crate::facade::{
    ExportFileSource, ExportFileSourcePort, FileExportDestination, ResourceFacadeError,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uc_core::clipboard::{
    FileDisplayMetadata, FileDisplayMetadataEntry, PersistedClipboardRepresentation,
};
use uc_core::ids::{EventId, FormatId, RepresentationId};
use uc_core::{MimeType, FILE_DISPLAY_METADATA_FORMAT, FILE_DISPLAY_METADATA_MIME};

struct Sources(HashMap<PathBuf, (u64, Vec<u8>)>);
#[async_trait]
impl ExportFileSourcePort for Sources {
    async fn open(&self, path: &Path) -> io::Result<ExportFileSource> {
        let (length, bytes) = self
            .0
            .get(path)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        Ok(ExportFileSource {
            length: *length,
            reader: Box::new(Cursor::new(bytes.clone())),
        })
    }
}

#[derive(Default)]
struct Destination {
    bytes: Cursor<Vec<u8>>,
    completed: bool,
    failure: Option<&'static str>,
    max_write: usize,
}
impl Write for Destination {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failure == Some("write") {
            return Err(io::ErrorKind::StorageFull.into());
        }
        self.max_write = self.max_write.max(bytes.len());
        self.bytes.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for Destination {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        if self.failure == Some("seek") {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.bytes.seek(from)
    }
}
impl FileExportDestination for Destination {
    fn ensure_open(&self) -> io::Result<()> {
        if self.failure == Some("cancel") {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        Ok(())
    }
    fn complete(&mut self) -> io::Result<()> {
        if self.failure == Some("complete") {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.completed = true;
        Ok(())
    }
}

fn fixtures() -> (TestDeps, Sources) {
    let deps = test_deps();
    seed_entry_with_file_rep(&deps, "files", Path::new("/archive/0"));
    let values = [
        ("报告 空格.txt", b"first".to_vec()),
        ("报告 空格.txt", vec![3; 180_000]),
        ("empty.txt", vec![]),
    ];
    let metadata = FileDisplayMetadata {
        files: values
            .iter()
            .enumerate()
            .map(|(i, (name, _))| FileDisplayMetadataEntry {
                storage_name: i.to_string(),
                display_name: (*name).to_owned(),
            })
            .collect(),
    }
    .encode()
    .unwrap();
    let mut representations = deps.representation_repo.by_event_id.lock().unwrap();
    let reps = representations.get_mut(&EventId::from("event-1")).unwrap();
    reps[0].inline_data =
        Some(b"file:///archive/0\nfile:///archive/1\nfile:///archive/2\n".to_vec());
    reps.push(PersistedClipboardRepresentation::new(
        RepresentationId::from("names"),
        FormatId::from(FILE_DISPLAY_METADATA_FORMAT),
        Some(MimeType(FILE_DISPLAY_METADATA_MIME.to_owned())),
        metadata.len() as i64,
        Some(metadata),
        None,
    ));
    drop(representations);
    let sources = Sources(
        values
            .into_iter()
            .enumerate()
            .map(|(i, (_, bytes))| {
                (
                    PathBuf::from(format!("/archive/{i}")),
                    (bytes.len() as u64, bytes),
                )
            })
            .collect(),
    );
    (deps, sources)
}

#[tokio::test]
async fn exports_every_member_with_names_empty_files_and_bounded_writes() {
    let (deps, sources) = fixtures();
    let mut destination = Destination::default();
    deps.facade
        .export_entry_files("files", &sources, &mut destination)
        .await
        .unwrap();
    assert!(destination.completed);
    assert!(destination.max_write <= 65536);
    let mut archive = zip::ZipArchive::new(destination.bytes).unwrap();
    assert_eq!(archive.len(), 3);
    for (name, expected) in [
        ("报告 空格.txt", b"first".to_vec()),
        ("报告 空格 (2).txt", vec![3; 180_000]),
        ("empty.txt", vec![]),
    ] {
        let mut actual = Vec::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_end(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn missing_last_source_never_creates_a_partial_success() {
    let (deps, mut sources) = fixtures();
    sources.0.remove(Path::new("/archive/2"));
    let mut destination = Destination::default();
    let error = deps
        .facade
        .export_entry_files("files", &sources, &mut destination)
        .await
        .unwrap_err();
    assert_io_source(error, io::ErrorKind::NotFound);
    assert!(!destination.completed);
    assert!(destination.bytes.into_inner().is_empty());
}

fn assert_io_source(error: ResourceFacadeError, expected: io::ErrorKind) {
    let ResourceFacadeError::Internal(source) = error else {
        panic!("expected preserved source")
    };
    assert!(source.chain().any(|error| error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == expected)));
}

#[tokio::test]
async fn destination_failures_and_cancellation_are_not_success() {
    for (failure, kind) in [
        ("write", io::ErrorKind::StorageFull),
        ("seek", io::ErrorKind::PermissionDenied),
        ("complete", io::ErrorKind::BrokenPipe),
        ("cancel", io::ErrorKind::ConnectionAborted),
    ] {
        let (deps, sources) = fixtures();
        let mut destination = Destination {
            failure: Some(failure),
            ..Default::default()
        };
        let error = deps
            .facade
            .export_entry_files("files", &sources, &mut destination)
            .await
            .unwrap_err();
        assert_io_source(error, kind);
        assert!(!destination.completed);
    }
}

#[tokio::test]
async fn changed_source_length_is_rejected() {
    for (length, expected) in [
        (2, io::ErrorKind::InvalidData),
        (99, io::ErrorKind::UnexpectedEof),
    ] {
        let (deps, mut sources) = fixtures();
        sources.0.get_mut(Path::new("/archive/0")).unwrap().0 = length;
        let mut destination = Destination::default();
        let error = deps
            .facade
            .export_entry_files("files", &sources, &mut destination)
            .await
            .unwrap_err();
        assert_io_source(error, expected);
        assert!(!destination.completed);
    }
}

#[tokio::test]
async fn corrupt_metadata_or_nonlocal_uri_fails_without_writing() {
    for invalid_metadata in [true, false] {
        let (deps, sources) = fixtures();
        {
            let mut representations = deps.representation_repo.by_event_id.lock().unwrap();
            let reps = representations.get_mut(&EventId::from("event-1")).unwrap();
            if invalid_metadata {
                reps[1].inline_data = Some(b"broken".to_vec());
            } else {
                reps[0].inline_data = Some(b"https://example.com/file".to_vec());
            }
        }
        let mut destination = Destination::default();
        assert!(deps
            .facade
            .export_entry_files("files", &sources, &mut destination)
            .await
            .is_err());
        assert!(!destination.completed);
        assert!(destination.bytes.into_inner().is_empty());
    }
}
