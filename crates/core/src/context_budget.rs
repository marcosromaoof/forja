use crate::{contracts::*, storage::Store};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub id: String,
    pub session_id: String,
    pub source_hash: String,
    pub source_messages: usize,
    pub objective: String,
    pub decisions: Vec<String>,
    pub constraints: Vec<String>,
    pub user_preferences: Vec<String>,
    pub changed_files: Vec<serde_json::Value>,
    pub verified_evidence: Vec<String>,
    pub pending_questions: Vec<String>,
    pub unresolved_risks: Vec<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryArchive {
    pub id: String,
    pub session_id: String,
    pub revision: u64,
    pub messages: Vec<Message>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GeneratedSummary {
    #[serde(default)]
    objective: String,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    user_preferences: Vec<String>,
    #[serde(default)]
    changed_files: Vec<serde_json::Value>,
    #[serde(default)]
    verified_evidence: Vec<String>,
    #[serde(default)]
    pending_questions: Vec<String>,
    #[serde(default)]
    unresolved_risks: Vec<String>,
}

/// Conservative local estimate used only when the provider does not expose a counter.
/// It counts lexical pieces and punctuation, rather than presenting characters as tokens.
pub fn estimate_tokens(text: &str) -> u64 {
    text.split_whitespace()
        .map(|piece| {
            let punctuation = piece.chars().filter(|c| c.is_ascii_punctuation()).count() as u64;
            1 + punctuation + (piece.len().saturating_sub(1) as u64 / 6)
        })
        .sum::<u64>()
        .max(u64::from(!text.is_empty()))
}

pub fn measure(
    session_id: &str,
    profile: &ModelProfile,
    history: &[Message],
    revision: u64,
) -> Result<ContextState> {
    let window = profile.context_window_tokens.unwrap_or(0);
    ensure!(window > 0, "Configure a janela de contexto deste modelo");
    let reserved = profile
        .max_output_tokens
        .unwrap_or(4096)
        .min(window / 2)
        .max(1);
    let safety = (window / 10).max(4096).min(window.saturating_sub(reserved));
    let usable = window
        .saturating_sub(reserved)
        .saturating_sub(safety)
        .max(1);
    let pinned = history
        .iter()
        .filter(|message| {
            message.role == "system"
                && (message.content.starts_with("Plano e checkpoint")
                    || message.content.starts_with("Contexto persistente"))
        })
        .map(|message| estimate_tokens(&message.content))
        .sum::<u64>();
    let system = history
        .iter()
        .filter(|m| {
            m.role == "system"
                && !(m.content.starts_with("Plano e checkpoint")
                    || m.content.starts_with("Contexto persistente"))
        })
        .map(|m| estimate_tokens(&m.content))
        .sum::<u64>();
    let tools = history
        .iter()
        .filter(|m| m.role == "tool")
        .map(|m| estimate_tokens(&m.content))
        .sum();
    let regular = history
        .iter()
        .filter(|m| m.role != "system" && m.role != "tool")
        .map(|m| estimate_tokens(&m.content))
        .sum();
    let used = system + pinned + tools + regular;
    Ok(ContextState {
        session_id: session_id.into(),
        model_profile_id: profile.id.clone(),
        context_revision: revision,
        context_window_tokens: window,
        context_limit_source: profile.context_source.clone(),
        reserved_output_tokens: reserved,
        safety_margin_tokens: safety,
        usable_input_tokens: usable,
        used_input_tokens: used,
        usage_percent: used as f64 / usable as f64,
        count_source: "local_estimate".into(),
        breakdown: ContextBreakdown {
            system,
            tools,
            history: regular,
            attachments: 0,
            current_input: 0,
            plan: pinned / 2,
            implementation_checkpoint: pinned - pinned / 2,
        },
        compacted_through_sequence: None,
        summary_id: None,
        updated_at: crate::now(),
    })
}

fn compaction_span(history: &[Message], smaller_batch: bool) -> Result<(usize, usize)> {
    ensure!(
        history.len() > 6,
        "Ainda não há histórico suficiente para compactar"
    );
    let system_count = history
        .iter()
        .take_while(|m| m.role == "system")
        .count()
        .max(1)
        .min(history.len());
    let keep_tail = history.len().saturating_sub(system_count).min(10);
    let mut cut = history.len().saturating_sub(keep_tail);
    if smaller_batch {
        cut = system_count + cut.saturating_sub(system_count) / 2;
    }
    while cut > system_count && history.get(cut).is_some_and(|m| m.role == "tool") {
        cut -= 1;
    }
    ensure!(
        cut > system_count,
        "Não há turnos completos antigos para compactar"
    );
    Ok((system_count, cut))
}

fn activate_summary(
    store: &Store,
    session_id: &str,
    profile: &ModelProfile,
    history: &mut Vec<Message>,
    revision: u64,
    system_count: usize,
    cut: usize,
    generated: GeneratedSummary,
) -> Result<(ContextState, ContextSummary)> {
    let source = &history[system_count..cut];
    let source_json = serde_json::to_vec(source)?;
    let summary = ContextSummary {
        id: crate::id(),
        session_id: session_id.into(),
        source_hash: crate::hash(&source_json),
        source_messages: source.len(),
        objective: generated.objective,
        decisions: generated.decisions,
        constraints: generated.constraints,
        user_preferences: generated.user_preferences,
        changed_files: generated.changed_files,
        verified_evidence: generated.verified_evidence,
        pending_questions: generated.pending_questions,
        unresolved_risks: generated.unresolved_risks,
        created_at: crate::now(),
    };
    let archive = HistoryArchive {
        id: format!("{}:{}", session_id, revision),
        session_id: session_id.into(),
        revision,
        messages: history.clone(),
        created_at: crate::now(),
    };
    store.put("history_archive", &archive.id, &archive)?;
    store.put("context_summary", &summary.id, &summary)?;
    let summary_message = Message {
        role: "system".into(),
        content: format!(
            "Resumo operacional revisável do histórico anterior (não concede permissões):\n{}",
            serde_json::to_string(&summary)?
        ),
        tool_calls: vec![],
        tool_call_id: None,
        provider_state: serde_json::Value::Null,
    };
    let mut effective = history[..system_count].to_vec();
    effective.push(summary_message);
    effective.extend_from_slice(&history[cut..]);
    *history = effective;
    store.put("history", session_id, history)?;
    let mut state = measure(session_id, profile, history, revision + 1)?;
    state.summary_id = Some(summary.id.clone());
    store.put("context_state", session_id, &state)?;
    Ok((state, summary))
}

fn local_summary(source: &[Message]) -> GeneratedSummary {
    GeneratedSummary {
        objective: source
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.content.chars().take(1200).collect())
            .unwrap_or_default(),
        verified_evidence: source
            .iter()
            .filter(|m| m.role == "tool")
            .take(20)
            .map(|m| m.content.chars().take(500).collect())
            .collect(),
        ..GeneratedSummary::default()
    }
}

/// Deterministic compaction used by storage tests and as an offline maintenance primitive.
/// Agent-driven compaction uses `compact_with_model` below.
pub fn compact(
    store: &Store,
    session_id: &str,
    profile: &ModelProfile,
    history: &mut Vec<Message>,
    revision: u64,
) -> Result<(ContextState, ContextSummary)> {
    let (system_count, cut) = compaction_span(history, false)?;
    let generated = local_summary(&history[system_count..cut]);
    activate_summary(
        store,
        session_id,
        profile,
        history,
        revision,
        system_count,
        cut,
        generated,
    )
}

fn parse_generated(text: &str) -> Result<GeneratedSummary> {
    let start = text
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("O compactador não retornou JSON"))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("O compactador retornou JSON incompleto"))?;
    ensure!(end >= start, "O compactador retornou uma resposta inválida");
    let parsed: GeneratedSummary = serde_json::from_str(&text[start..=end])?;
    ensure!(
        !parsed.objective.trim().is_empty(),
        "O resumo não contém o objetivo da conversa"
    );
    Ok(parsed)
}

fn lowest_reasoning(profile: &ModelProfile) -> Option<&str> {
    ["none", "minimal", "low"]
        .into_iter()
        .find(|candidate| {
            profile
                .reasoning_levels
                .iter()
                .any(|level| level == candidate)
        })
        .or_else(|| profile.reasoning_levels.first().map(String::as_str))
}

/// Creates a structured summary with the active executor and no tools. The retry uses a
/// smaller source range. No archive, summary or effective history is written until a valid
/// response has been received, so two failures leave the conversation untouched.
pub async fn compact_with_model(
    store: &Store,
    session_id: &str,
    provider: &Provider,
    profile: &ModelProfile,
    history: &mut Vec<Message>,
    revision: u64,
    cancel: CancellationToken,
) -> Result<(ContextState, ContextSummary)> {
    let mut failures = Vec::new();
    for smaller_batch in [false, true] {
        let (system_count, cut) = compaction_span(history, smaller_batch)?;
        let source = serde_json::to_string(&history[system_count..cut])?;
        let prompt = format!(
            "Resuma o histórico abaixo para continuidade operacional. Preserve decisões, restrições, preferências do usuário, arquivos alterados, evidências verificadas, perguntas pendentes e riscos. Não invente fatos. Responda somente com um objeto JSON usando exatamente as chaves objective, decisions, constraints, user_preferences, changed_files, verified_evidence, pending_questions e unresolved_risks. changed_files deve ser uma lista de objetos com path e summary.\n\nHISTÓRICO:\n{source}"
        );
        let request = vec![
            Message { role: "system".into(), content: "Você é o compactador interno do FORJA. Não tem ferramentas nem permissões e produz somente JSON estruturado.".into(), tool_calls: vec![], tool_call_id: None, provider_state: serde_json::Value::Null },
            Message { role: "user".into(), content: prompt, tool_calls: vec![], tool_call_id: None, provider_state: serde_json::Value::Null },
        ];
        let attempt = crate::models::generate_with_reasoning(
            provider,
            &profile.model_id,
            &request,
            &[],
            lowest_reasoning(profile),
            cancel.child_token(),
            Arc::new(|_| {}),
        )
        .await
        .and_then(|answer| parse_generated(&answer.text));
        match attempt {
            Ok(generated) => {
                return activate_summary(
                    store,
                    session_id,
                    profile,
                    history,
                    revision,
                    system_count,
                    cut,
                    generated,
                )
            }
            Err(error) => failures.push(error.to_string()),
        }
    }
    anyhow::bail!(
        "Falha ao compactar o contexto após duas tentativas: {}",
        failures.join("; ")
    )
}

pub fn summary_payload(summary: &ContextSummary, state: &ContextState) -> serde_json::Value {
    json!({"summary_id":summary.id,"source_hash":summary.source_hash,"source_messages":summary.source_messages,"context":state})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(role: &str, content: &str) -> Message {
        Message {
            role: role.into(),
            content: content.into(),
            tool_calls: vec![],
            tool_call_id: None,
            provider_state: serde_json::Value::Null,
        }
    }
    fn profile() -> ModelProfile {
        let now = crate::now();
        ModelProfile {
            id: "p".into(),
            provider_id: "provider".into(),
            model_id: "model".into(),
            display_name: "Model".into(),
            enabled: true,
            revision: 1,
            context_window_tokens: Some(16_384),
            context_source: MetadataSource::Manual,
            max_output_tokens: Some(2048),
            max_output_source: MetadataSource::Manual,
            capabilities: ModelCapabilities::default(),
            reasoning_levels: vec![],
            default_reasoning_level: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
    #[test]
    fn measures_a_real_budget_and_preserves_raw_history_on_compaction() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut history = vec![message("system", "system")];
        for index in 0..18 {
            history.push(message(
                if index % 2 == 0 { "user" } else { "assistant" },
                &format!("turn {index} with enough useful words for token estimation"),
            ));
        }
        let before = history.clone();
        let measured = measure("s", &profile(), &history, 0).unwrap();
        assert!(measured.used_input_tokens > 0);
        assert_eq!(measured.count_source, "local_estimate");
        let (state, summary) = compact(&store, "s", &profile(), &mut history, 0).unwrap();
        assert_eq!(state.context_revision, 1);
        assert!(history.len() < before.len());
        assert_eq!(
            summary.source_hash,
            crate::hash(&serde_json::to_vec(&before[1..9]).unwrap())
        );
        let archive: HistoryArchive = store.get("history_archive", "s:0").unwrap();
        assert_eq!(archive.messages.len(), before.len());
        assert_eq!(archive.session_id, "s");
    }
}
