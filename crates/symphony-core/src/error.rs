use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymphonyError {
    #[error("missing_workflow_file: {0}")]
    MissingWorkflowFile(String),
    #[error("workflow_parse_error: {0}")]
    WorkflowParseError(String),
    #[error("workflow_front_matter_not_a_map")]
    WorkflowFrontMatterNotAMap,
    #[error("template_parse_error: {0}")]
    TemplateParseError(String),
    #[error("template_render_error: {0}")]
    TemplateRenderError(String),
    #[error("config_invalid: {0}")]
    ConfigInvalid(String),
    #[error("unsupported_tracker_kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("missing_tracker_api_key")]
    MissingTrackerApiKey,
    #[error("missing_tracker_project_slug")]
    MissingTrackerProjectSlug,
    #[error("linear_api_request: {0}")]
    LinearApiRequest(String),
    #[error("linear_api_status: {0}")]
    LinearApiStatus(u16),
    #[error("linear_graphql_errors: {0}")]
    LinearGraphqlErrors(String),
    #[error("linear_unknown_payload")]
    LinearUnknownPayload,
    #[error("linear_missing_end_cursor")]
    LinearMissingEndCursor,
    #[error("invalid_workspace_cwd: {0}")]
    InvalidWorkspaceCwd(String),
    #[error("workspace_outside_root")]
    WorkspaceOutsideRoot,
    #[error("hook_failed: {hook}: {reason}")]
    HookFailed { hook: String, reason: String },
    #[error("hook_timeout: {hook}")]
    HookTimeout { hook: String },
    #[error("agent_not_found: {0}")]
    AgentNotFound(String),
    #[error("turn_timeout")]
    TurnTimeout,
    #[error("turn_failed: {0}")]
    TurnFailed(String),
    #[error("turn_input_required")]
    TurnInputRequired,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SymphonyError>;
