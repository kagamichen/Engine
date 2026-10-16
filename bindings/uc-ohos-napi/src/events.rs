use napi_derive::napi;
use uc_engine::{TransferDirectionSummary, TransferProgress};

#[napi(object)]
pub struct OhTransferProgress {
    pub transfer_id: String,
    pub entry_id: Option<String>,
    pub attempt_id: Option<String>,
    pub peer_id: String,
    pub direction: String,
    pub completed_bytes: String,
    pub total_bytes: Option<String>,
}

#[napi(object)]
pub struct OhTransferStatus {
    pub transfer_id: String,
    pub entry_id: Option<String>,
    pub attempt_id: Option<String>,
    pub status: String,
    pub reason: Option<String>,
}

#[napi(object)]
pub struct OhReceiveAttempt {
    pub entry_id: String,
    pub attempt_id: String,
    pub state: String,
}

#[napi(object)]
pub struct OhPeerPresence {
    pub device_id: String,
    pub state: String,
    pub at_ms: f64,
}

pub(crate) fn transfer_progress(progress: TransferProgress) -> OhTransferProgress {
    OhTransferProgress {
        transfer_id: progress.transfer_id,
        entry_id: progress.entry_id,
        attempt_id: progress.attempt_id,
        peer_id: progress.peer_id,
        direction: match progress.direction {
            TransferDirectionSummary::Sending => "sending",
            TransferDirectionSummary::Receiving => "receiving",
        }
        .to_owned(),
        completed_bytes: progress.completed_bytes.to_string(),
        total_bytes: progress.total_bytes.map(|bytes| bytes.to_string()),
    }
}
