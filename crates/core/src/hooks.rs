use crate::{contracts::ToolCall, policy, storage::Store};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

#[derive(Debug, Serialize)]
pub struct EventDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub tool_filter: bool,
}
pub const EVENTS: &[EventDefinition] = &[
    EventDefinition {
        id: "before_tool",
        label: "Antes da ferramenta",
        tool_filter: true,
    },
    EventDefinition {
        id: "after_tool",
        label: "Depois da ferramenta",
        tool_filter: true,
    },
    EventDefinition {
        id: "tool_error",
        label: "Quando a ferramenta falhar",
        tool_filter: true,
    },
    EventDefinition {
        id: "before_model_request",
        label: "Antes de consultar o modelo",
        tool_filter: false,
    },
    EventDefinition {
        id: "after_model_response",
        label: "Depois da resposta do modelo",
        tool_filter: false,
    },
    EventDefinition {
        id: "before_final",
        label: "Antes de concluir a execução",
        tool_filter: false,
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub event: String,
    pub tools: Vec<String>,
    pub command: String,
    pub cwd: String,
    pub timeout_seconds: u64,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    #[serde(default)]
    pub position: u32,
    #[serde(default = "revision_default")]
    pub revision: u64,
}
fn enabled_default() -> bool {
    true
}
fn revision_default() -> u64 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub id: String,
    pub revision: u64,
    pub position: u32,
}
impl Hook {
    pub fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 120,
            "Nome do hook inválido"
        );
        let event = EVENTS
            .iter()
            .find(|e| e.id == self.event)
            .ok_or_else(|| anyhow::anyhow!("Evento de hook inválido"))?;
        ensure!(
            !self.command.trim().is_empty()
                && self.command.len() <= 16_000
                && !self.command.contains('\0'),
            "Comando do hook inválido"
        );
        ensure!(
            (1..=300).contains(&self.timeout_seconds),
            "Timeout deve estar entre 1 e 300 segundos"
        );
        ensure!(
            if event.tool_filter { !self.tools.is_empty() && self.tools.len() <= 12 } else { self.tools.is_empty() },
            "Eventos de ferramenta exigem uma seleção; eventos do modelo e conclusão não aceitam filtro de ferramentas"
        );
        let available: HashSet<_> = policy::tools().into_iter().map(|t| t.name).collect();
        let unique: HashSet<_> = self.tools.iter().collect();
        ensure!(
            unique.len() == self.tools.len() && self.tools.iter().all(|t| available.contains(t)),
            "Ferramenta do hook inválida ou duplicada"
        );
        let cwd = policy::resolve(root, &self.cwd, false)?;
        ensure!(!self.enabled || cwd.is_dir(), "Diretório do hook inválido");
        Ok(())
    }
    pub fn hash(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        // Reordering affects future runs only. Content revisions revoke old approvals.
        value.as_object_mut().unwrap().remove("position");
        Ok(crate::hash(&serde_json::to_vec(&value)?))
    }
    pub fn approval(&self, call: Option<&ToolCall>) -> Result<ToolCall> {
        Ok(ToolCall {
            id: crate::id(),
            name: "hook.exec".into(),
            arguments: serde_json::json!({
                "hook_id":self.id,"hook_name":self.name,"config_hash":self.hash()?,
                "event":self.event,"event_label":EVENTS.iter().find(|e|e.id==self.event).map(|e|e.label),
                "trigger_tool":call.map(|c|&c.name),"trigger_call_id":call.map(|c|&c.id),
                "command":self.command,"cwd":self.cwd,"timeout_seconds":self.timeout_seconds,
                "access":"native_process_reduced_isolation"
            }),
        })
    }
}
pub fn list(store: &Store, workspace: &str) -> Result<Vec<Hook>> {
    let mut hooks: Vec<_> = store
        .list::<Hook>("hook")?
        .into_iter()
        .filter(|h| h.workspace_id == workspace)
        .collect();
    sort(&mut hooks);
    ensure!(hooks.len() <= 16, "Limite de 16 hooks por projeto");
    Ok(hooks)
}
fn sort(hooks: &mut [Hook]) {
    hooks.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
}
pub fn create(store: &Store, root: &Path, mut hook: Hook) -> Result<Hook> {
    hook.validate(root)?;
    hook.id = crate::id();
    hook.revision = 1;
    store.edit_documents("hook", |all: &mut Vec<Hook>| {
        let existing: Vec<_> = all
            .iter()
            .filter(|h| h.workspace_id == hook.workspace_id)
            .collect();
        ensure!(existing.len() < 16, "Limite de 16 hooks por projeto");
        hook.position = existing
            .iter()
            .map(|h| h.position)
            .max()
            .map_or(Ok(0), |p| {
                p.checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("Reordene os hooks antes de criar outro"))
            })?;
        all.push(hook.clone());
        Ok(hook)
    })
}
pub fn update(store: &Store, root: &Path, mut hook: Hook, expected_revision: u64) -> Result<Hook> {
    hook.validate(root)?;
    store.edit_documents("hook", |all: &mut Vec<Hook>| {
        let current = all
            .iter_mut()
            .find(|h| h.id == hook.id && h.workspace_id == hook.workspace_id)
            .ok_or_else(|| anyhow::anyhow!("Hook não encontrado neste projeto"))?;
        ensure!(
            current.revision == expected_revision,
            "Hook alterado em outra janela. Atualize a lista e revise suas alterações."
        );
        hook.position = current.position;
        hook.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Limite de versões atingido"))?;
        *current = hook.clone();
        Ok(hook)
    })
}
pub fn remove(store: &Store, workspace: &str, id: &str, expected_revision: u64) -> Result<()> {
    store.edit_documents("hook", |all: &mut Vec<Hook>| {
        let index = all
            .iter()
            .position(|h| h.id == id && h.workspace_id == workspace)
            .ok_or_else(|| anyhow::anyhow!("Hook não encontrado neste projeto"))?;
        ensure!(
            all[index].revision == expected_revision,
            "Hook alterado em outra janela. Atualize a lista antes de remover."
        );
        all.remove(index);
        Ok(())
    })
}
pub fn reorder(
    store: &Store,
    workspace: &str,
    ids: &[String],
    expected: &[Version],
) -> Result<Vec<Hook>> {
    store.edit_documents("hook", |all: &mut Vec<Hook>| {
        let mut current: Vec<_> = all
            .iter()
            .filter(|h| h.workspace_id == workspace)
            .cloned()
            .collect();
        sort(&mut current);
        let versions: Vec<_> = current
            .iter()
            .map(|h| Version {
                id: h.id.clone(),
                revision: h.revision,
                position: h.position,
            })
            .collect();
        ensure!(
            versions == expected,
            "A lista mudou em outra janela. Atualize antes de reordenar."
        );
        let unique: HashSet<_> = ids.iter().collect();
        ensure!(
            ids.len() == current.len()
                && unique.len() == ids.len()
                && current.iter().all(|h| unique.contains(&h.id)),
            "Informe todos os hooks deste projeto exatamente uma vez"
        );
        for hook in all.iter_mut().filter(|h| h.workspace_id == workspace) {
            hook.position = ids.iter().position(|id| id == &hook.id).unwrap() as u32;
        }
        let mut result: Vec<_> = all
            .iter()
            .filter(|h| h.workspace_id == workspace)
            .cloned()
            .collect();
        sort(&mut result);
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Hook {
        serde_json::from_value(serde_json::json!({"id":"legacy","workspace_id":"w","name":"Check","event":"before_final","tools":[],"command":"echo ok","cwd":".","timeout_seconds":5})).unwrap()
    }
    fn versions(hooks: &[Hook]) -> Vec<Version> {
        hooks
            .iter()
            .map(|h| Version {
                id: h.id.clone(),
                revision: h.revision,
                position: h.position,
            })
            .collect()
    }
    #[test]
    fn legacy_defaults_and_order_are_compatible_with_frozen_approvals() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let legacy = sample();
        assert!(legacy.enabled);
        assert_eq!(legacy.revision, 1);
        assert_eq!(legacy.position, 0);
        let first = create(&store, root.path(), legacy).unwrap();
        let second = create(&store, root.path(), sample()).unwrap();
        let before = list(&store, "w").unwrap();
        let old_hash = first.hash().unwrap();
        let next = reorder(
            &store,
            "w",
            &[second.id.clone(), first.id.clone()],
            &versions(&before),
        )
        .unwrap();
        assert_eq!(next[0].id, second.id);
        assert_eq!(next[1].hash().unwrap(), old_hash);
        assert!(reorder(
            &store,
            "w",
            &[first.id.clone(), second.id.clone()],
            &versions(&before)
        )
        .is_err());
        assert!(reorder(
            &store,
            "w",
            &[first.id.clone(), first.id.clone()],
            &versions(&next)
        )
        .is_err());
        assert_eq!(versions(&list(&store, "w").unwrap()), versions(&next));
        let mut changed = first.clone();
        changed.enabled = false;
        let changed = update(&store, root.path(), changed, first.revision).unwrap();
        assert_eq!(changed.position, 1);
        assert_eq!(changed.revision, 2);
        assert_ne!(changed.hash().unwrap(), old_hash);
        assert!(remove(&store, "w", &changed.id, 1).is_err());
        assert!(remove(&store, "other", &changed.id, 2).is_err());
        remove(&store, "w", &changed.id, 2).unwrap();
        assert_eq!(list(&store, "w").unwrap().len(), 1);
    }
    #[test]
    fn simultaneous_edits_have_one_winner_and_no_lost_update() {
        use std::sync::{Arc, Barrier};
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(data.path()).unwrap());
        let original = create(&store, root.path(), sample()).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|n| {
                let store = store.clone();
                let barrier = barrier.clone();
                let mut hook = original.clone();
                let path = root.path().to_owned();
                std::thread::spawn(move || {
                    hook.name = format!("Editor {n}");
                    barrier.wait();
                    update(&store, &path, hook, 1)
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let stored: Hook = store.get("hook", &original.id).unwrap();
        assert_eq!(stored.revision, 2);
        assert_eq!(
            stored.name,
            results.iter().find_map(|r| r.as_ref().ok()).unwrap().name
        );
        let mut wrong = stored.clone();
        wrong.workspace_id = "other".into();
        assert!(update(&store, root.path(), wrong, 2).is_err());
    }
    #[test]
    fn disabled_hook_can_be_saved_when_its_working_directory_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let mut hook = sample();
        hook.cwd = "missing".into();
        hook.enabled = false;
        hook.validate(root.path()).unwrap();
        hook.enabled = true;
        assert!(hook.validate(root.path()).is_err());
        hook.enabled = false;
        hook.cwd = "../outside".into();
        assert!(hook.validate(root.path()).is_err());
    }
    #[test]
    fn lifecycle_events_reject_tool_filters_and_keep_approvals_explicit() {
        let root = tempfile::tempdir().unwrap();
        for event in EVENTS {
            let mut hook = Hook {
                id: "h".into(),
                workspace_id: "w".into(),
                name: "Check".into(),
                event: event.id.into(),
                tools: if event.tool_filter {
                    vec!["fs.read_text".into()]
                } else {
                    vec![]
                },
                command: "echo ok".into(),
                cwd: ".".into(),
                timeout_seconds: 5,
                enabled: true,
                position: 0,
                revision: 1,
            };
            hook.validate(root.path()).unwrap();
            if !event.tool_filter {
                let approval = hook.approval(None).unwrap();
                assert!(approval.arguments["trigger_tool"].is_null());
                assert_eq!(approval.arguments["event_label"], event.label);
                hook.tools.push("fs.read_text".into());
            } else {
                hook.tools.clear();
            }
            assert!(hook.validate(root.path()).is_err());
        }
    }
    #[test]
    fn config_cannot_broaden_policy_or_inject_recursive_hooks() {
        let root = tempfile::tempdir().unwrap();
        let mut h = Hook {
            id: "h".into(),
            workspace_id: "w".into(),
            name: "Check".into(),
            event: "before_tool".into(),
            tools: vec!["fs.read_text".into()],
            command: "echo ok".into(),
            cwd: ".".into(),
            timeout_seconds: 5,
            enabled: true,
            position: 0,
            revision: 1,
        };
        h.validate(root.path()).unwrap();
        let call = h
            .approval(Some(&ToolCall {
                id: "t".into(),
                name: "fs.read_text".into(),
                arguments: serde_json::json!({}),
            }))
            .unwrap();
        assert!(policy::validate(&call).is_err()); // Internal approvals are never model tools.
        assert_eq!(
            policy::evaluate(&crate::contracts::Mode::Agent, &call, false),
            policy::Decision::Deny
        );
        h.tools = vec!["hook.exec".into()];
        assert!(h.validate(root.path()).is_err());
        h.tools = vec!["fs.read_text".into()];
        h.cwd = "../outside".into();
        assert!(h.validate(root.path()).is_err());
    }
}
