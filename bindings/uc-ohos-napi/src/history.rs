use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use uc_engine::{
    BlobResourceInput, EntryNotResendableReason, HistoryEntryInput, Operation, OperationResult,
    ResendEntryInput, ResendEntryOutcome,
};

use crate::runtime::{count, engine_error, unexpected_result};
use crate::OhEngine;

#[napi(object)]
pub struct OhHistoryDetail {
    pub entry_id: String,
    pub content: String,
    pub size_bytes: String,
    pub created_at_ms: f64,
    pub active_time_ms: f64,
    pub mime_type: Option<String>,
}

#[napi(object)]
pub struct OhHistoryResource {
    pub blob_id: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: String,
    pub url: Option<String>,
    pub inline_data: Option<Uint8Array>,
}

#[napi(object)]
pub struct OhBinaryResource {
    pub bytes: Uint8Array,
    pub media_type: Option<String>,
}

#[napi(object)]
pub struct OhDeliveryCounts {
    pub accepted: u32,
    pub duplicate: u32,
    pub offline: u32,
    pub errored: u32,
    pub pending: u32,
}

#[napi(object)]
pub struct OhResendOutcome {
    pub kind: String,
    pub report: Option<OhDeliveryCounts>,
    pub entry_id: Option<String>,
    pub device_id: Option<String>,
    pub reason: Option<String>,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn get_history_entry(&self, entry_id: String) -> napi::Result<OhHistoryDetail> {
        match self
            .engine
            .execute(Operation::GetHistoryEntry(HistoryEntryInput { entry_id }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::HistoryEntry(entry) => Ok(OhHistoryDetail {
                entry_id: entry.entry_id,
                content: entry.content,
                size_bytes: entry.size_bytes.to_string(),
                created_at_ms: entry.created_at_ms as f64,
                active_time_ms: entry.active_time_ms as f64,
                mime_type: entry.mime_type,
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn delete_history_entry(&self, entry_id: String) -> napi::Result<()> {
        match self
            .engine
            .execute(Operation::DeleteHistoryEntry(HistoryEntryInput {
                entry_id,
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::HistoryEntryDeleted => Ok(()),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn get_history_entry_resource(
        &self,
        entry_id: String,
    ) -> napi::Result<OhHistoryResource> {
        match self
            .engine
            .execute(Operation::GetHistoryEntryResource(HistoryEntryInput {
                entry_id,
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::HistoryEntryResource(resource) => Ok(OhHistoryResource {
                blob_id: resource.blob_id,
                mime_type: resource.mime_type,
                size_bytes: resource.size_bytes.to_string(),
                url: resource.url,
                inline_data: resource.inline_data.map(Uint8Array::from),
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn read_blob(&self, blob_id: String) -> napi::Result<OhBinaryResource> {
        match self
            .engine
            .execute(Operation::ReadBlob(BlobResourceInput { blob_id }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::BlobRead(resource) => Ok(OhBinaryResource {
                bytes: resource.bytes.into(),
                media_type: resource.media_type,
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn resend_entry(
        &self,
        entry_id: String,
        target_devices: Vec<String>,
    ) -> napi::Result<OhResendOutcome> {
        match self
            .engine
            .execute(Operation::ResendEntry(ResendEntryInput {
                entry_id,
                target_devices,
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::EntryResent(outcome) => resend_outcome(outcome),
            _ => Err(unexpected_result()),
        }
    }
}

fn resend_outcome(outcome: ResendEntryOutcome) -> napi::Result<OhResendOutcome> {
    let mut mapped = OhResendOutcome {
        kind: String::new(),
        report: None,
        entry_id: None,
        device_id: None,
        reason: None,
    };
    mapped.kind = match outcome {
        ResendEntryOutcome::Completed(report) => {
            mapped.report = Some(OhDeliveryCounts {
                accepted: count(report.accepted)?,
                duplicate: count(report.duplicate)?,
                offline: count(report.offline)?,
                errored: count(report.errored)?,
                pending: count(report.pending)?,
            });
            "completed"
        }
        ResendEntryOutcome::SynchronizationDisabled => "synchronization_disabled",
        ResendEntryOutcome::EntryNotFound { entry_id } => {
            mapped.entry_id = Some(entry_id);
            "entry_not_found"
        }
        ResendEntryOutcome::EntryNotResendable { entry_id, reason } => {
            mapped.entry_id = Some(entry_id);
            mapped.reason = Some(
                match reason {
                    EntryNotResendableReason::RemoteOrigin => "remote_origin",
                    EntryNotResendableReason::PayloadLost => "payload_lost",
                }
                .to_owned(),
            );
            "entry_not_resendable"
        }
        ResendEntryOutcome::TargetNotTrusted { device_id } => {
            mapped.device_id = Some(device_id);
            "target_not_trusted"
        }
        ResendEntryOutcome::NoEligibleTargets => "no_eligible_targets",
    }
    .to_owned();
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resend_preserves_rejection_context_without_fake_delivery_counts() {
        let mapped = resend_outcome(ResendEntryOutcome::EntryNotResendable {
            entry_id: "fixture".into(),
            reason: EntryNotResendableReason::RemoteOrigin,
        })
        .unwrap();
        assert_eq!(mapped.kind, "entry_not_resendable");
        assert_eq!(mapped.reason.as_deref(), Some("remote_origin"));
        assert!(mapped.report.is_none());
    }
}
