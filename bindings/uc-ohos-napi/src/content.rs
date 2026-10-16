use napi_derive::napi;
use uc_engine::{
    ContentTypesPatch, ContentTypesSummary, MemberSyncPreferencesPatch,
    ObserveClipboardChangeInput, Operation, OperationResult, QueryHistoryInput,
    QueryMemberSyncPreferencesInput, UpdateMemberSyncPreferencesInput,
};

use crate::runtime::{engine_error, send_report, unexpected_result};
use crate::{OhEngine, OhSendReport};

#[napi(object)]
pub struct OhHistoryEntry {
    pub entry_id: String,
    pub content_type: String,
    pub preview: Option<String>,
    pub created_at_ms: f64,
}

#[napi(object)]
pub struct OhHistoryPage {
    pub entries: Vec<OhHistoryEntry>,
    pub next_cursor: Option<String>,
}

#[napi(object)]
pub struct OhDevice {
    pub device_id: String,
    pub display_name: String,
    pub is_local: bool,
    pub online: bool,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn query_history(
        &self,
        cursor: Option<String>,
        limit: u32,
        query: Option<String>,
    ) -> napi::Result<OhHistoryPage> {
        match self
            .engine
            .execute(Operation::QueryHistory(QueryHistoryInput {
                cursor,
                limit,
                query,
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::HistoryPage {
                entries,
                next_cursor,
            } => Ok(OhHistoryPage {
                entries: entries
                    .into_iter()
                    .map(|entry| OhHistoryEntry {
                        entry_id: entry.entry_id,
                        content_type: entry.content_type,
                        preview: entry.preview,
                        created_at_ms: entry.created_at_ms as f64,
                    })
                    .collect(),
                next_cursor,
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn list_devices(&self) -> napi::Result<Vec<OhDevice>> {
        match self
            .engine
            .execute(Operation::ListDevices)
            .await
            .map_err(engine_error)?
        {
            OperationResult::Devices(devices) => Ok(devices
                .into_iter()
                .map(|device| OhDevice {
                    device_id: device.device_id,
                    display_name: device.display_name,
                    is_local: device.is_local,
                    online: device.online,
                })
                .collect()),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn observe_clipboard_change(
        &self,
        dispatch: bool,
    ) -> napi::Result<Option<OhSendReport>> {
        match self
            .engine
            .execute(Operation::ObserveClipboardChange(
                ObserveClipboardChangeInput { dispatch },
            ))
            .await
            .map_err(engine_error)?
        {
            OperationResult::ClipboardChangeObserved { report } => {
                report.map(send_report).transpose()
            }
            _ => Err(unexpected_result()),
        }
    }
}

#[napi(object)]
pub struct OhContentTypes {
    pub text: bool,
    pub image: bool,
    pub link: bool,
    pub file: bool,
    pub code_snippet: bool,
    pub rich_text: bool,
}

#[napi(object)]
pub struct OhSyncPreferences {
    pub send_enabled: bool,
    pub receive_enabled: bool,
    pub send_content_types: OhContentTypes,
    pub receive_content_types: OhContentTypes,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn query_member_sync_preferences(
        &self,
        device_id: String,
    ) -> napi::Result<OhSyncPreferences> {
        let result = self
            .engine
            .execute(Operation::QueryMemberSyncPreferences(
                QueryMemberSyncPreferencesInput { device_id },
            ))
            .await
            .map_err(engine_error)?;
        preferences_result(result)
    }

    #[napi]
    pub async fn update_member_sync_preferences(
        &self,
        device_id: String,
        preferences: OhSyncPreferences,
    ) -> napi::Result<OhSyncPreferences> {
        let result = self
            .engine
            .execute(Operation::UpdateMemberSyncPreferences(
                UpdateMemberSyncPreferencesInput {
                    device_id,
                    patch: MemberSyncPreferencesPatch {
                        send_enabled: Some(preferences.send_enabled),
                        receive_enabled: Some(preferences.receive_enabled),
                        send_content_types: Some(content_types_patch(
                            preferences.send_content_types,
                        )),
                        receive_content_types: Some(content_types_patch(
                            preferences.receive_content_types,
                        )),
                    },
                },
            ))
            .await
            .map_err(engine_error)?;
        preferences_result(result)
    }
}

fn content_types_patch(types: OhContentTypes) -> ContentTypesPatch {
    ContentTypesPatch {
        text: Some(types.text),
        image: Some(types.image),
        link: Some(types.link),
        file: Some(types.file),
        code_snippet: Some(types.code_snippet),
        rich_text: Some(types.rich_text),
    }
}

fn content_types(types: ContentTypesSummary) -> OhContentTypes {
    OhContentTypes {
        text: types.text,
        image: types.image,
        link: types.link,
        file: types.file,
        code_snippet: types.code_snippet,
        rich_text: types.rich_text,
    }
}

fn preferences_result(result: OperationResult) -> napi::Result<OhSyncPreferences> {
    match result {
        OperationResult::MemberSyncPreferences(summary) => Ok(OhSyncPreferences {
            send_enabled: summary.send_enabled,
            receive_enabled: summary.receive_enabled,
            send_content_types: content_types(summary.send_content_types),
            receive_content_types: content_types(summary.receive_content_types),
        }),
        _ => Err(unexpected_result()),
    }
}
