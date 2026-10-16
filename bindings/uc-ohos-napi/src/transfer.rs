use napi_derive::napi;
use uc_engine::{
    CancelEntryReceiveInput, CancelInboundTransferInput, EntryReceiveCancellationOutcome,
    EntryReceiveProgressInput, InboundTransferCancellationOutcome, Operation, OperationResult,
    ReceiveProgressSummary, TransferCancellationReason,
};

use crate::runtime::{engine_error, unexpected_result};
use crate::OhEngine;

#[napi(object)]
pub struct OhReceiveProgress {
    pub entry_id: String,
    pub attempt_id: String,
    pub state: String,
    pub total_bytes: String,
    pub completed_bytes: String,
    pub items_total: u32,
    pub items_completed: u32,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn query_entry_receive_progress(
        &self,
        entry_id: String,
    ) -> napi::Result<Option<OhReceiveProgress>> {
        match self
            .engine
            .execute(Operation::QueryEntryReceiveProgress(
                EntryReceiveProgressInput { entry_id },
            ))
            .await
            .map_err(engine_error)?
        {
            OperationResult::EntryReceiveProgress(progress) => Ok(progress.map(receive_progress)),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn list_entry_receive_progress(&self) -> napi::Result<Vec<OhReceiveProgress>> {
        match self
            .engine
            .execute(Operation::ListEntryReceiveProgress)
            .await
            .map_err(engine_error)?
        {
            OperationResult::EntryReceiveProgressList(progress) => {
                Ok(progress.into_iter().map(receive_progress).collect())
            }
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn cancel_entry_receive(
        &self,
        entry_id: String,
        attempt_id: String,
    ) -> napi::Result<String> {
        match self
            .engine
            .execute(Operation::CancelEntryReceive(CancelEntryReceiveInput {
                entry_id,
                attempt_id,
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::EntryReceiveCancellation(outcome) => {
                Ok(cancellation(outcome).to_owned())
            }
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn cancel_inbound_transfer(&self, transfer_id: String) -> napi::Result<String> {
        match self
            .engine
            .execute(Operation::CancelInboundTransfer(
                CancelInboundTransferInput {
                    transfer_id,
                    reason: TransferCancellationReason::LocalUser,
                },
            ))
            .await
            .map_err(engine_error)?
        {
            OperationResult::InboundTransferCancellation(outcome) => Ok(match outcome {
                InboundTransferCancellationOutcome::Cancelled => "cancelled",
                InboundTransferCancellationOutcome::NotInflight => "not_inflight",
            }
            .to_owned()),
            _ => Err(unexpected_result()),
        }
    }
}

fn cancellation(outcome: EntryReceiveCancellationOutcome) -> &'static str {
    match outcome {
        EntryReceiveCancellationOutcome::Cancelled => "cancelled",
        EntryReceiveCancellationOutcome::NotReceiving => "not_receiving",
        EntryReceiveCancellationOutcome::TooLate => "too_late",
        EntryReceiveCancellationOutcome::AlreadyTerminal => "already_terminal",
        EntryReceiveCancellationOutcome::Superseded => "superseded",
    }
}

fn receive_progress(progress: ReceiveProgressSummary) -> OhReceiveProgress {
    OhReceiveProgress {
        entry_id: progress.entry_id,
        attempt_id: progress.attempt_id,
        state: progress.state,
        total_bytes: progress.total_bytes.to_string(),
        completed_bytes: progress.completed_bytes.to_string(),
        items_total: progress.items_total,
        items_completed: progress.items_completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_preserves_large_byte_counts_and_attempt_identity() {
        let mapped = receive_progress(ReceiveProgressSummary {
            entry_id: "entry".into(),
            attempt_id: "new-attempt".into(),
            state: "receiving".into(),
            total_bytes: i64::MAX,
            completed_bytes: i64::MAX - 1,
            items_total: 2,
            items_completed: 1,
        });
        assert_eq!(mapped.total_bytes, "9223372036854775807");
        assert_eq!(mapped.completed_bytes, "9223372036854775806");
        assert_eq!(mapped.attempt_id, "new-attempt");
    }

    #[test]
    fn cancellation_preserves_every_distinct_business_outcome() {
        for (outcome, expected) in [
            (EntryReceiveCancellationOutcome::Cancelled, "cancelled"),
            (
                EntryReceiveCancellationOutcome::NotReceiving,
                "not_receiving",
            ),
            (EntryReceiveCancellationOutcome::TooLate, "too_late"),
            (
                EntryReceiveCancellationOutcome::AlreadyTerminal,
                "already_terminal",
            ),
            (EntryReceiveCancellationOutcome::Superseded, "superseded"),
        ] {
            assert_eq!(cancellation(outcome), expected);
        }
    }
}
