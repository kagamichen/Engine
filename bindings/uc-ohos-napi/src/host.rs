use std::path::PathBuf;

use napi::bindgen_prelude::{FromNapiValue, Uint8Array, ValidateNapiValue};
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction};
use napi::{Env, JsObject, Status};
use uc_engine::observability::{io_error_kind, uc_warn};
use uc_engine::{
    HostCapabilities, HostCapabilityError, HostCapabilityErrorCategory, HostClipboard,
    HostClipboardRepresentation, HostClipboardSnapshot, HostDirectories, HostFileAccess,
    HostFileHandle, HostFileMetadata, HostSecureStorage,
};

use crate::{OhClipboardRepresentation, OhClipboardSnapshot, OhHost};

mod callback;

use callback::call_host;

pub(crate) fn unref_callbacks(host: &mut OhHost, env: &Env) -> napi::Result<()> {
    host.secure_storage_get.unref(env)?;
    host.secure_storage_set.unref(env)?;
    host.secure_storage_delete.unref(env)?;
    host.file_metadata.unref(env)?;
    host.file_read_chunk.unref(env)?;
    host.file_write_chunk.unref(env)?;
    host.file_finish_write.unref(env)?;
    host.clipboard_read.unref(env)?;
    host.clipboard_write.unref(env)
}

pub(crate) fn capabilities(host: OhHost) -> napi::Result<HostCapabilities> {
    let cache_directory = PathBuf::from(host.cache_directory);
    let directories = HostDirectories::new(
        PathBuf::from(host.private_data_directory),
        cache_directory.clone(),
        PathBuf::from(host.temporary_directory),
        cache_directory.join("logs"),
    );
    for directory in [
        directories.private_data(),
        directories.cache(),
        directories.temporary(),
    ] {
        std::fs::create_dir_all(directory).map_err(host_directory_create_failed)?;
    }
    Ok(HostCapabilities::new(
        directories,
        Box::new(OhSecureStorage {
            get: host.secure_storage_get,
            set: host.secure_storage_set,
            delete: host.secure_storage_delete,
        }),
        Box::new(OhClipboard {
            read: host.clipboard_read,
            write: host.clipboard_write,
        }),
        Box::new(OhFiles {
            metadata: host.file_metadata,
            read_chunk: host.file_read_chunk,
            write_chunk: host.file_write_chunk,
            finish_write: host.file_finish_write,
        }),
    ))
}

/// 宿主目录无法创建；JS 侧只收到稳定错误码，路径不进入记录。
fn host_directory_create_failed(error: std::io::Error) -> napi::Error {
    uc_warn!(
        operation = "host_directory_create",
        error_kind = "host_io",
        io_error_kind = io_error_kind(&error),
        "host operation failed"
    );
    host_error("create host directory")
}

fn host_error(operation: &str) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("OHOS_HOST_IO:{operation}"))
}

fn host_result(result: &JsObject) -> Result<(), HostCapabilityError> {
    if property::<bool>(result, "ok")? {
        return Ok(());
    }
    let category = match property::<String>(result, "errorCategory")?.as_str() {
        "permission_denied" => HostCapabilityErrorCategory::PermissionDenied,
        "invalid_handle" => HostCapabilityErrorCategory::InvalidHandle,
        "unavailable" => HostCapabilityErrorCategory::Unavailable,
        _ => HostCapabilityErrorCategory::Io,
    };
    Err(HostCapabilityError::new(
        category,
        "HarmonyOS host capability failed",
    ))
}

struct OhSecureStorage {
    get: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
    set: ThreadsafeFunction<(String, Uint8Array), ErrorStrategy::Fatal>,
    delete: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
}

impl HostSecureStorage for OhSecureStorage {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, HostCapabilityError> {
        call_host(&self.get, key.to_owned(), |result| {
            host_result(&result)?;
            Ok(property::<Option<Uint8Array>>(&result, "value")?.map(|bytes| bytes.to_vec()))
        })
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<(), HostCapabilityError> {
        call_host(
            &self.set,
            (key.to_owned(), Uint8Array::from(value.to_vec())),
            |result| host_result(&result),
        )
    }

    fn delete(&self, key: &str) -> Result<(), HostCapabilityError> {
        call_host(&self.delete, key.to_owned(), |result| host_result(&result))
    }
}

struct OhClipboard {
    read: ThreadsafeFunction<(), ErrorStrategy::Fatal>,
    write: ThreadsafeFunction<OhClipboardSnapshot, ErrorStrategy::Fatal>,
}

impl HostClipboard for OhClipboard {
    fn read(&self) -> Result<HostClipboardSnapshot, HostCapabilityError> {
        call_host(&self.read, (), |result| {
            host_result(&result)?;
            clipboard_snapshot_from_js(property::<JsObject>(&result, "value")?)
        })
    }

    fn write(&self, snapshot: HostClipboardSnapshot) -> Result<(), HostCapabilityError> {
        call_host(
            &self.write,
            OhClipboardSnapshot {
                observed_at_ms: snapshot.observed_at_ms as f64,
                representations: snapshot
                    .representations
                    .into_iter()
                    .map(clipboard_representation_to_host)
                    .collect(),
            },
            |result| host_result(&result),
        )
    }
}

fn clipboard_snapshot_from_js(
    snapshot: JsObject,
) -> Result<HostClipboardSnapshot, HostCapabilityError> {
    let observed_at_ms = property::<f64>(&snapshot, "observedAtMs")?;
    let representations = property::<Vec<JsObject>>(&snapshot, "representations")?;
    Ok(HostClipboardSnapshot {
        observed_at_ms: parse_safe_i64(observed_at_ms)?,
        representations: representations
            .into_iter()
            .map(clipboard_representation_from_js)
            .collect::<Result<_, _>>()?,
    })
}

fn clipboard_representation_from_js(
    representation: JsObject,
) -> Result<HostClipboardRepresentation, HostCapabilityError> {
    let kind = property::<String>(&representation, "kind")?;
    let format = property::<String>(&representation, "format")?;
    let mime_type = property::<Option<String>>(&representation, "mimeType")?;
    match kind.as_str() {
        "inline" => Ok(HostClipboardRepresentation::Inline {
            format,
            mime_type,
            bytes: property::<Uint8Array>(&representation, "bytes")?.to_vec(),
        }),
        "file" => Ok(HostClipboardRepresentation::File {
            format,
            handle: HostFileHandle::new(property::<String>(&representation, "handle")?),
            display_name: property::<String>(&representation, "displayName")?,
            mime_type,
            size_bytes: parse_u64(&property::<String>(&representation, "sizeBytes")?)?,
        }),
        _ => Err(host_contract_error()),
    }
}

fn clipboard_representation_to_host(
    representation: HostClipboardRepresentation,
) -> OhClipboardRepresentation {
    match representation {
        HostClipboardRepresentation::Inline {
            format,
            mime_type,
            bytes,
        } => OhClipboardRepresentation {
            kind: "inline".to_owned(),
            format,
            mime_type,
            bytes: Some(Uint8Array::from(bytes)),
            handle: None,
            display_name: None,
            size_bytes: None,
        },
        HostClipboardRepresentation::File {
            format,
            handle,
            display_name,
            mime_type,
            size_bytes,
        } => OhClipboardRepresentation {
            kind: "file".to_owned(),
            format,
            mime_type,
            bytes: None,
            handle: Some(handle.as_str().to_owned()),
            display_name: Some(display_name),
            size_bytes: Some(size_bytes.to_string()),
        },
    }
}

fn parse_safe_i64(value: f64) -> Result<i64, HostCapabilityError> {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    if value.is_finite()
        && value.fract() == 0.0
        && (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
    {
        return Ok(value as i64);
    }
    Err(host_contract_error())
}

fn host_contract_error() -> HostCapabilityError {
    HostCapabilityError::new(
        HostCapabilityErrorCategory::Io,
        "HarmonyOS host returned an invalid capability value",
    )
}

fn property<T>(object: &JsObject, name: &str) -> Result<T, HostCapabilityError>
where
    T: FromNapiValue + ValidateNapiValue,
{
    object
        .get_named_property(name)
        // discarded-source[input-validation]: the rejection reason is fully expressed by the target classification
        .map_err(|_| host_contract_error())
}

struct OhFiles {
    metadata: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
    read_chunk: ThreadsafeFunction<(String, String, u32), ErrorStrategy::Fatal>,
    write_chunk: ThreadsafeFunction<(String, String, Uint8Array), ErrorStrategy::Fatal>,
    finish_write: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
}

impl HostFileAccess for OhFiles {
    fn metadata(&self, handle: &HostFileHandle) -> Result<HostFileMetadata, HostCapabilityError> {
        call_host(&self.metadata, handle.as_str().to_owned(), |result| {
            host_result(&result)?;
            file_metadata_from_js(property::<JsObject>(&result, "value")?)
        })
    }

    fn read_chunk(
        &self,
        handle: &HostFileHandle,
        offset: u64,
        max_bytes: u32,
    ) -> Result<Vec<u8>, HostCapabilityError> {
        call_host(
            &self.read_chunk,
            (handle.as_str().to_owned(), offset.to_string(), max_bytes),
            |result| {
                host_result(&result)?;
                Ok(property::<Uint8Array>(&result, "value")?.to_vec())
            },
        )
    }

    fn write_chunk(
        &self,
        handle: &HostFileHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), HostCapabilityError> {
        call_host(
            &self.write_chunk,
            (
                handle.as_str().to_owned(),
                offset.to_string(),
                Uint8Array::from(bytes.to_vec()),
            ),
            |result| host_result(&result),
        )
    }

    fn finish_write(&self, handle: &HostFileHandle) -> Result<(), HostCapabilityError> {
        call_host(&self.finish_write, handle.as_str().to_owned(), |result| {
            host_result(&result)
        })
    }
}

fn file_metadata_from_js(metadata: JsObject) -> Result<HostFileMetadata, HostCapabilityError> {
    Ok(HostFileMetadata {
        display_name: property(&metadata, "displayName")?,
        size_bytes: parse_u64(&property::<String>(&metadata, "sizeBytes")?)?,
        mime_type: property(&metadata, "mimeType")?,
    })
}

fn parse_u64(value: &str) -> Result<u64, HostCapabilityError> {
    // discarded-source[input-validation]: `core::num::ParseIntError`: the rejection reason is fully expressed by the target classification
    value.parse().map_err(|_| {
        HostCapabilityError::new(
            HostCapabilityErrorCategory::InvalidHandle,
            "HarmonyOS host returned an invalid file size or offset",
        )
    })
}
