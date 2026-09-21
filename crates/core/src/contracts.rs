use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Consult,
    Plan,
    Agent,
    Review,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub mode: Mode,
    pub provider_id: String,
    pub model: String,
    pub created_at: String,
    #[serde(default = "crate::now")]
    pub updated_at: String,
    #[serde(default)]
    pub executor_profile_id: Option<String>,
    #[serde(default)]
    pub reviewer_profile_ids: Vec<String>,
    #[serde(default)]
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub last_run_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub schema_version: u32,
    pub session_id: String,
    pub run_id: String,
    pub sequence: i64,
    #[serde(default)]
    pub session_sequence: i64,
    pub timestamp: String,
    pub r#type: String,
    pub payload: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub local_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub risk: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub id: String,
    pub session_id: String,
    pub run_id: String,
    pub tool: ToolCall,
    pub scope_key: String,
    pub state: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub session_id: String,
    pub goal: String,
    pub state: String,
    pub created_at: String,
    pub max_turns: u32,
    #[serde(default)]
    pub selected_skills: Vec<crate::skills::Selection>,
    #[serde(default)]
    pub mode: Option<Mode>,
    #[serde(default)]
    pub executor_profile_id: Option<String>,
    #[serde(default)]
    pub reviewer_profile_ids: Vec<String>,
    #[serde(default)]
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub context_revision: u64,
    #[serde(default)]
    pub resumed_from_run_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRunOptions {
    pub mode: Mode,
    #[serde(default)]
    pub executor_profile_id: Option<String>,
    #[serde(default)]
    pub reviewer_profile_ids: Vec<String>,
    #[serde(default)]
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub context_revision: u64,
    #[serde(default)]
    pub resumed_from_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    Provider,
    Probe,
    Manual,
    Unknown,
}
impl Default for MetadataSource {
    fn default() -> Self {
        Self::Unknown
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySupport {
    pub state: String,
    #[serde(default)]
    pub source: MetadataSource,
}
impl Default for CapabilitySupport {
    fn default() -> Self {
        Self {
            state: "unknown".into(),
            source: MetadataSource::Unknown,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub text: CapabilitySupport,
    pub vision: CapabilitySupport,
    pub tools: CapabilitySupport,
    pub structured_output: CapabilitySupport,
    pub reasoning: CapabilitySupport,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub revision: u64,
    pub context_window_tokens: Option<u64>,
    #[serde(default)]
    pub context_source: MetadataSource,
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub max_output_source: MetadataSource,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub reasoning_levels: Vec<String>,
    #[serde(default)]
    pub default_reasoning_level: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBreakdown {
    pub system: u64,
    pub tools: u64,
    pub history: u64,
    pub attachments: u64,
    pub current_input: u64,
    #[serde(default)]
    pub plan: u64,
    #[serde(default)]
    pub implementation_checkpoint: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextState {
    pub session_id: String,
    pub model_profile_id: String,
    pub context_revision: u64,
    pub context_window_tokens: u64,
    pub context_limit_source: MetadataSource,
    pub reserved_output_tokens: u64,
    pub safety_margin_tokens: u64,
    pub usable_input_tokens: u64,
    pub used_input_tokens: u64,
    pub usage_percent: f64,
    pub count_source: String,
    pub breakdown: ContextBreakdown,
    pub compacted_through_sequence: Option<i64>,
    pub summary_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub id: String,
    pub session_id: String,
    pub run_id: String,
    pub question: String,
    #[serde(default)]
    pub detail: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub options: Vec<InteractionOption>,
    pub required: bool,
    #[serde(default)]
    pub allow_custom: bool,
    pub state: String,
    #[serde(default)]
    pub answer: Value,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub directory: bool,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub provider_state: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Value,
    pub correlation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub url: String,
    pub token: String,
    pub pid: u32,
    pub protocol: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub order: u32,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub expected_files: Vec<String>,
    #[serde(default)]
    pub validation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifact {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub source_run_id: String,
    pub revision: u64,
    pub state: String,
    pub title: String,
    pub summary: String,
    pub objective: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub risks: Vec<String>,
    pub source_hash: String,
    pub markdown_artifact_id: String,
    pub markdown_path: String,
    #[serde(default)]
    pub implementation_run_id: Option<String>,
    #[serde(default)]
    pub baseline_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineFile {
    pub path: String,
    pub hash: String,
    #[serde(default)]
    pub blob_id: Option<String>,
    #[serde(default)]
    pub excluded_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitBaseline {
    pub repository: bool,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
    pub status: String,
    #[serde(default)]
    pub staged_diff_blob: Option<String>,
    #[serde(default)]
    pub unstaged_diff_blob: Option<String>,
    #[serde(default)]
    pub untracked_files: Vec<BaselineFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationBaseline {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub plan_id: String,
    pub plan_revision: u64,
    pub run_id: String,
    #[serde(default)]
    pub git: Option<GitBaseline>,
    pub workspace_manifest_hash: String,
    pub context_revision: u64,
    pub session_sequence: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressStep {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub evidence_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFileEvidence {
    pub path: String,
    #[serde(default)]
    pub before_hash: Option<String>,
    pub after_hash: String,
    #[serde(default)]
    pub file_checkpoint_ids: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationEvidence {
    #[serde(default)]
    pub command: Option<String>,
    pub description: String,
    pub state: String,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationCheckpoint {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub plan_id: String,
    pub plan_revision: u64,
    pub root_run_id: String,
    pub revision: u64,
    pub reason: String,
    pub status: String,
    #[serde(default)]
    pub current_step_id: Option<String>,
    pub steps: Vec<ProgressStep>,
    #[serde(default)]
    pub completed_work: Vec<String>,
    #[serde(default)]
    pub pending_work: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub unresolved_risks: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<ChangedFileEvidence>,
    #[serde(default)]
    pub validations: Vec<ValidationEvidence>,
    #[serde(default)]
    pub active_agent_task_ids: Vec<String>,
    #[serde(default)]
    pub browser_artifact_ids: Vec<String>,
    pub first_session_sequence: i64,
    pub last_session_sequence: i64,
    pub source_events_hash: String,
    #[serde(default)]
    pub previous_checkpoint_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRevisionProposal {
    pub id: String,
    pub plan_id: String,
    pub from_revision: u64,
    pub proposed_revision: u64,
    pub reason: String,
    pub changed_sections: Vec<String>,
    pub markdown_diff: String,
    pub state: String,
    pub created_by_run_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub role: String,
    pub instructions: String,
    pub model_profile_id: String,
    #[serde(default)]
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub write_access: String,
    pub max_turns: u32,
    #[serde(default)]
    pub token_budget: Option<u64>,
    pub time_budget_seconds: u64,
    pub created_by: String,
    #[serde(default)]
    pub created_by_run_id: Option<String>,
    pub enabled: bool,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub parent_run_id: String,
    pub agent_profile_id: String,
    pub objective: String,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub step_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub state: String,
    #[serde(default)]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub result_artifact_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeProposal {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub run_id: String,
    pub path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub base_hash: String,
    pub old_text: String,
    pub new_text: String,
    #[serde(default)]
    pub explanation: Option<String>,
    pub state: String,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserGrant {
    pub id: String,
    pub session_id: String,
    pub allowed_origins: Vec<String>,
    pub capabilities: Vec<String>,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub kind: String,
    pub media_type: String,
    pub blob_id: String,
    pub metadata: Value,
    pub created_at: String,
}
