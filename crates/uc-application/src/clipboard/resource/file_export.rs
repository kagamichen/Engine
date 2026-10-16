use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Context;
use tokio::io::AsyncReadExt;
use uc_core::clipboard::FileDisplayMetadata;
use uc_core::ids::EntryId;
use url::Url;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{
    is_file_display_metadata_representation, is_file_list_representation, ResourceFacade,
    ResourceFacadeError,
};

mod names;
mod ports;

use names::archive_name;
pub use ports::{ExportFileSource, ExportFileSourcePort, FileExportDestination};

const COPY_BLOCK_BYTES: usize = 64 * 1024;

impl ResourceFacade {
    /// 将记录中的全部文件流式写入 ZIP；任何成员失败都不能完成目标。
    pub async fn export_entry_files(
        &self,
        entry_id: &str,
        sources: &dyn ExportFileSourcePort,
        destination: &mut dyn FileExportDestination,
    ) -> Result<(), ResourceFacadeError> {
        destination
            .ensure_open()
            .context("check export destination")
            .map_err(ResourceFacadeError::Internal)?;
        let files = self.export_file_list(entry_id).await?;
        let mut opened = Vec::with_capacity(files.len());
        let mut used = HashSet::new();
        for (path, name) in files {
            destination
                .ensure_open()
                .context("check export destination")
                .map_err(ResourceFacadeError::Internal)?;
            let source = sources
                .open(&path)
                .await
                .context("open export source")
                .map_err(ResourceFacadeError::Internal)?;
            opened.push((archive_name(&name, &mut used), source));
        }
        let mut archive = ZipWriter::new(&mut *destination);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        let mut buffer = vec![0_u8; COPY_BLOCK_BYTES];
        for (name, mut source) in opened {
            archive
                .start_file(name, options)
                .context("start archived file")
                .map_err(ResourceFacadeError::Internal)?;
            let mut copied = 0_u64;
            loop {
                let count = source
                    .reader
                    .read(&mut buffer)
                    .await
                    .context("read export source")
                    .map_err(ResourceFacadeError::Internal)?;
                if count == 0 {
                    break;
                }
                copied = copied.checked_add(count as u64).ok_or_else(|| {
                    ResourceFacadeError::Internal(anyhow::anyhow!("export source length overflow"))
                })?;
                if copied > source.length {
                    return Err(ResourceFacadeError::Internal(
                        io::Error::new(io::ErrorKind::InvalidData, "export source length changed")
                            .into(),
                    ));
                }
                archive
                    .write_all(&buffer[..count])
                    .context("write archived file")
                    .map_err(ResourceFacadeError::Internal)?;
                tokio::task::yield_now().await;
            }
            if copied != source.length {
                return Err(ResourceFacadeError::Internal(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "export source was truncated")
                        .into(),
                ));
            }
        }
        archive
            .finish()
            .context("finish archive")
            .map_err(ResourceFacadeError::Internal)?;
        destination
            .complete()
            .context("complete export destination")
            .map_err(ResourceFacadeError::Internal)
    }

    async fn export_file_list(
        &self,
        entry_id: &str,
    ) -> Result<Vec<(PathBuf, String)>, ResourceFacadeError> {
        let entry = self
            .deps
            .entry_repo
            .get_entry(&EntryId::from(entry_id))
            .await
            .context("read export entry")
            .map_err(ResourceFacadeError::Internal)?
            .ok_or(ResourceFacadeError::NotFound)?;
        let representations = self
            .deps
            .representations_for_event
            .get_representations_for_event(&entry.event_id)
            .await
            .context("read export representations")
            .map_err(ResourceFacadeError::Internal)?;
        let file_rep = representations
            .iter()
            .find(|rep| is_file_list_representation(rep))
            .ok_or(ResourceFacadeError::NotFound)?;
        let data = file_rep
            .inline_data
            .as_deref()
            .ok_or(ResourceFacadeError::NotFound)?;
        let list = std::str::from_utf8(data)
            .context("decode export file list")
            .map_err(ResourceFacadeError::Internal)?;
        let metadata = representations
            .iter()
            .find(|rep| is_file_display_metadata_representation(rep))
            .map(|rep| {
                let bytes = rep
                    .inline_data
                    .as_deref()
                    .ok_or(ResourceFacadeError::NotFound)?;
                FileDisplayMetadata::decode(bytes)
                    .context("decode export file names")
                    .map_err(ResourceFacadeError::Internal)
            })
            .transpose()?;
        let mut files = Vec::new();
        for line in list
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let uri = Url::parse(line)
                .context("parse export file URI")
                .map_err(ResourceFacadeError::Internal)?;
            // discarded-source[no-information]: URL 转换只返回 ()，不携带底层失败信息。
            let path = uri.to_file_path().map_err(|()| {
                ResourceFacadeError::Mismatch("export source is not a local file".to_owned())
            })?;
            let storage_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file");
            let name = metadata
                .as_ref()
                .and_then(|names| names.display_name_for(storage_name))
                .unwrap_or(storage_name)
                .to_owned();
            files.push((path, name));
        }
        if files.is_empty() {
            return Err(ResourceFacadeError::NotFound);
        }
        Ok(files)
    }
}
