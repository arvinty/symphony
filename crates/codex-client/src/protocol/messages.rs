use crate::error::RequestId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{v1, v2};

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Response {
        #[serde(default)]
        jsonrpc: String,
        id: RequestId,
        #[serde(flatten)]
        result: JsonRpcResult,
    },
    Request {
        #[serde(default)]
        jsonrpc: String,
        id: RequestId,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Notification {
        #[serde(default)]
        jsonrpc: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub enum JsonRpcResult {
    #[serde(rename = "result")]
    Ok(Value),
    #[serde(rename = "error")]
    Err(JsonRpcError),
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Requests the client (us) sends to the server. `#[serde(tag, content)]` produces
/// `{"method": "<name>", "params": <object>}`; the dispatcher overlays `jsonrpc`
/// and `id` at the JSON-RPC envelope layer.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    #[serde(rename = "initialize")]
    Initialize(v1::InitializeParams),

    #[serde(rename = "thread/start")]
    ThreadStart(v2::ThreadStartParams),

    #[serde(rename = "turn/start")]
    TurnStart(v2::TurnStartParams),

    #[serde(rename = "turn/interrupt")]
    TurnInterrupt(v2::TurnInterruptParams),

    #[serde(rename = "thread/approveGuardianDeniedAction")]
    ThreadApproveGuardianDeniedAction(v2::ThreadApproveGuardianDeniedActionParams),
}

/// Notifications the server emits. Untagged outer enum: serde tries each
/// variant in order — `Known` matches enumerated methods via tag/content,
/// and unmatched frames fall through to `Unknown` which preserves the raw
/// method + params for forensics.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ServerNotification {
    Known(KnownServerNotification),
    Unknown {
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum KnownServerNotification {
    #[serde(rename = "turn/started")]
    TurnStarted(v2::TurnStartedNotification),

    #[serde(rename = "turn/completed")]
    TurnCompleted(v2::TurnCompletedNotification),

    #[serde(rename = "turn/diff/updated")]
    TurnDiffUpdated(v2::TurnDiffUpdatedNotification),

    #[serde(rename = "turn/plan/updated")]
    TurnPlanUpdated(v2::TurnPlanUpdatedNotification),

    #[serde(rename = "item/started")]
    ItemStarted(v2::ItemStartedNotification),

    #[serde(rename = "item/completed")]
    ItemCompleted(v2::ItemCompletedNotification),

    #[serde(rename = "item/agentMessage/delta")]
    AgentMessageDelta(v2::AgentMessageDeltaNotification),

    #[serde(rename = "item/fileChange/patchUpdated")]
    FileChangePatchUpdated(v2::FileChangePatchUpdatedNotification),

    #[serde(rename = "item/autoApprovalReview/started")]
    AutoApprovalReviewStarted(v2::ItemGuardianApprovalReviewStartedNotification),

    #[serde(rename = "item/autoApprovalReview/completed")]
    AutoApprovalReviewCompleted(v2::ItemGuardianApprovalReviewCompletedNotification),

    #[serde(rename = "thread/tokenUsage/updated")]
    TokenUsageUpdated(v2::ThreadTokenUsageUpdatedNotification),

    #[serde(rename = "account/rateLimits/updated")]
    RateLimitsUpdated(v2::AccountRateLimitsUpdatedNotification),

    #[serde(rename = "error")]
    Error(v2::ErrorNotification),

    #[serde(rename = "warning")]
    Warning(v2::WarningNotification),

    #[serde(rename = "guardianWarning")]
    GuardianWarning(v2::GuardianWarningNotification),

    #[serde(rename = "deprecationNotice")]
    DeprecationNotice(v2::DeprecationNoticeNotification),

    #[serde(rename = "configWarning")]
    ConfigWarning(v2::ConfigWarningNotification),

    #[serde(rename = "mcpServer/startupStatus/updated")]
    McpServerStartupStatusUpdated(v2::McpServerStatusUpdatedNotification),
}
