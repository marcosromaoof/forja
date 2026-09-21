use crate::{contracts::*, files, storage::Store};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct DraftStep {
    id: String,
    title: String,
    description: String,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    expected_files: Vec<String>,
    validation: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DraftPlan {
    title: String,
    summary: String,
    objective: String,
    constraints: Vec<String>,
    decisions: Vec<String>,
    acceptance_criteria: Vec<String>,
    steps: Vec<DraftStep>,
    #[serde(default)]
    risks: Vec<String>,
}

fn text(value: &str, name: &str, max: usize) -> Result<String> {
    let value = value.trim();
    ensure!(!value.is_empty(), "{name} é obrigatório");
    ensure!(value.len() <= max, "{name} excede o limite");
    Ok(value.to_owned())
}

fn validate_graph(steps: &[PlanStep]) -> Result<()> {
    ensure!(
        !steps.is_empty() && steps.len() <= 100,
        "O plano deve ter de 1 a 100 etapas"
    );
    let ids: HashSet<_> = steps.iter().map(|step| step.id.as_str()).collect();
    ensure!(
        ids.len() == steps.len(),
        "Identificadores de etapa duplicados"
    );
    for step in steps {
        ensure!(
            step.id.len() <= 80
                && step
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
            "Identificador de etapa inválido"
        );
        for dependency in &step.dependencies {
            ensure!(
                ids.contains(dependency.as_str()) && dependency != &step.id,
                "Dependência de etapa inválida"
            );
        }
        for path in &step.expected_files {
            ensure!(
                !path.contains(':')
                    && !path.contains('\\')
                    && !path.starts_with('/')
                    && !path.split('/').any(|part| part == ".."),
                "Caminho esperado inválido"
            );
        }
    }
    fn visit<'a>(
        id: &'a str,
        map: &HashMap<&'a str, &'a PlanStep>,
        visiting: &mut HashSet<&'a str>,
        done: &mut HashSet<&'a str>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        ensure!(visiting.insert(id), "O plano contém dependência cíclica");
        for dependency in &map[id].dependencies {
            visit(dependency, map, visiting, done)?;
        }
        visiting.remove(id);
        done.insert(id);
        Ok(())
    }
    let map: HashMap<_, _> = steps.iter().map(|step| (step.id.as_str(), step)).collect();
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    for id in ids {
        visit(id, &map, &mut visiting, &mut done)?;
    }
    Ok(())
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 50 {
            break;
        }
    }
    out.trim_matches('-').to_owned().if_empty("plano")
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}

fn markdown(plan: &PlanArtifact) -> String {
    let list = |values: &[String]| -> String {
        if values.is_empty() {
            "- Nenhum item registrado.\n".into()
        } else {
            values
                .iter()
                .map(|value| format!("- {value}\n"))
                .collect::<String>()
        }
    };
    let steps: String = plan
        .steps
        .iter()
        .map(|step| {
            format!(
                "- [ ] **{} — {}**\n  {}\n  - Dependências: {}\n  - Validação: {}\n",
                step.id,
                step.title,
                step.description,
                if step.dependencies.is_empty() {
                    "nenhuma".into()
                } else {
                    step.dependencies.join(", ")
                },
                if step.validation.is_empty() {
                    "não definida".into()
                } else {
                    step.validation.join("; ")
                }
            )
        })
        .collect();
    format!("---\nforja_plan_id: \"{}\"\nrevision: {}\nworkspace_id: \"{}\"\nsession_id: \"{}\"\nstate: {}\nsource_hash: \"{}\"\ncreated_at: \"{}\"\n---\n\n# {}\n\n{}\n\n## Objetivo\n\n{}\n\n## Decisões\n\n{}\n## Restrições\n\n{}\n## Etapas\n\n{}\n## Critérios de aceite\n\n{}\n## Riscos\n\n{}", plan.id, plan.revision, plan.workspace_id, plan.session_id, plan.state, plan.source_hash, plan.created_at, plan.title, plan.summary, plan.objective, list(&plan.decisions), list(&plan.constraints), steps, list(&plan.acceptance_criteria), list(&plan.risks))
}

fn plan_dir(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let directory = root.join("docs").join("forja-plans");
    std::fs::create_dir_all(&directory)?;
    ensure!(
        directory.canonicalize()?.starts_with(&root),
        "Pasta de planos fora do projeto"
    );
    Ok(directory)
}

pub fn create(
    store: &Store,
    workspace: &Workspace,
    session: &Session,
    run: &Run,
    input: &Value,
) -> Result<PlanArtifact> {
    ensure!(
        run.mode.as_ref().unwrap_or(&session.mode) == &Mode::Plan,
        "Planos só podem ser apresentados no modo Planejamento"
    );
    let draft: DraftPlan = serde_json::from_value(input.clone())?;
    ensure!(
        !draft.acceptance_criteria.is_empty() && draft.acceptance_criteria.len() <= 100,
        "Informe critérios de aceite"
    );
    let steps: Vec<_> = draft
        .steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| {
            Ok(PlanStep {
                id: text(&step.id, "Identificador", 80)?,
                order: index as u32 + 1,
                title: text(&step.title, "Título da etapa", 300)?,
                description: text(&step.description, "Descrição da etapa", 5_000)?,
                dependencies: step.dependencies,
                expected_files: step.expected_files,
                validation: step.validation,
            })
        })
        .collect::<Result<_>>()?;
    validate_graph(&steps)?;
    let existing: Vec<PlanArtifact> = store
        .list("plan")?
        .into_iter()
        .filter(|plan: &PlanArtifact| plan.session_id == session.id)
        .collect();
    ensure!(!existing.iter().any(|plan| plan.state == "implementing"), "A implementação possui um plano ativo; proponha uma revisão estruturada em vez de substituí-lo");
    let revision = existing.iter().map(|plan| plan.revision).max().unwrap_or(0) + 1;
    for mut previous in existing
        .into_iter()
        .filter(|plan| matches!(plan.state.as_str(), "ready" | "revision_pending"))
    {
        previous.state = "superseded".into();
        previous.updated_at = crate::now();
        store.put("plan", &previous.id, &previous)?;
    }
    let id = crate::id();
    let semantic = json!({"title":draft.title,"summary":draft.summary,"objective":draft.objective,"constraints":draft.constraints,"decisions":draft.decisions,"acceptance_criteria":draft.acceptance_criteria,"steps":steps,"risks":draft.risks});
    let source_hash = crate::hash(&serde_json::to_vec(&semantic)?);
    let relative = format!(
        "docs/forja-plans/{}-{}.md",
        slug(semantic["title"].as_str().unwrap_or("plano")),
        &id[..8]
    );
    let created_at = crate::now();
    let mut plan = PlanArtifact {
        id: id.clone(),
        workspace_id: workspace.id.clone(),
        session_id: session.id.clone(),
        source_run_id: run.id.clone(),
        revision,
        state: "ready".into(),
        title: text(semantic["title"].as_str().unwrap_or(""), "Título", 300)?,
        summary: text(semantic["summary"].as_str().unwrap_or(""), "Resumo", 5_000)?,
        objective: text(
            semantic["objective"].as_str().unwrap_or(""),
            "Objetivo",
            10_000,
        )?,
        constraints: serde_json::from_value(semantic["constraints"].clone())?,
        decisions: serde_json::from_value(semantic["decisions"].clone())?,
        acceptance_criteria: serde_json::from_value(semantic["acceptance_criteria"].clone())?,
        steps: serde_json::from_value(semantic["steps"].clone())?,
        risks: serde_json::from_value(semantic["risks"].clone())?,
        source_hash,
        markdown_artifact_id: String::new(),
        markdown_path: relative.clone(),
        implementation_run_id: None,
        baseline_id: None,
        created_at: created_at.clone(),
        updated_at: created_at,
    };
    let bytes = markdown(&plan).into_bytes();
    let blob = store.blob(&bytes)?;
    plan.markdown_artifact_id = blob;
    let directory = plan_dir(Path::new(&workspace.root))?;
    let target = directory.join(
        Path::new(&relative)
            .file_name()
            .context("Nome de plano inválido")?,
    );
    files::atomic_create(&target, &bytes)
        .context("Não foi possível materializar o documento do plano")?;
    store.put("plan", &plan.id, &plan)?;
    Ok(plan)
}

pub fn active(store: &Store, session_id: &str) -> Result<Option<PlanArtifact>> {
    let mut plans: Vec<PlanArtifact> = store
        .list("plan")?
        .into_iter()
        .filter(|plan: &PlanArtifact| {
            plan.session_id == session_id && !matches!(plan.state.as_str(), "superseded")
        })
        .collect();
    plans.sort_by(|a, b| b.revision.cmp(&a.revision));
    Ok(plans.into_iter().next())
}

async fn git_or_empty(root: &Path, args: &[&str]) -> String {
    crate::process::git(root, args).await.unwrap_or_default()
}

pub async fn baseline(
    store: &Store,
    workspace: &Workspace,
    session: &Session,
    plan: &PlanArtifact,
    run_id: &str,
    context_revision: u64,
) -> Result<ImplementationBaseline> {
    let root = Path::new(&workspace.root);
    let repository = !git_or_empty(root, &["rev-parse", "--is-inside-work-tree"])
        .await
        .trim()
        .is_empty();
    let git = if repository {
        let status = git_or_empty(
            root,
            &["status", "--short", "--branch", "--untracked-files=all"],
        )
        .await;
        let branch = git_or_empty(root, &["branch", "--show-current"])
            .await
            .trim()
            .to_owned();
        let head = git_or_empty(root, &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_owned();
        let staged = git_or_empty(root, &["diff", "--cached", "--binary", "--no-ext-diff"]).await;
        let unstaged = git_or_empty(root, &["diff", "--binary", "--no-ext-diff"]).await;
        let mut total = 0usize;
        let mut untracked_files = Vec::new();
        for line in status
            .lines()
            .filter(|line| line.starts_with("?? "))
            .take(500)
        {
            let path = line[3..].trim_matches('"');
            match files::read(root, path) {
                Ok(file)
                    if file.content.len() <= 1_000_000
                        && total + file.content.len() <= 8_000_000 =>
                {
                    total += file.content.len();
                    untracked_files.push(BaselineFile {
                        path: path.into(),
                        hash: file.hash,
                        blob_id: Some(store.blob(file.content.as_bytes())?),
                        excluded_reason: None,
                    });
                }
                Ok(file) => untracked_files.push(BaselineFile {
                    path: path.into(),
                    hash: file.hash,
                    blob_id: None,
                    excluded_reason: Some("Arquivo excede o limite de baseline".into()),
                }),
                Err(error) => untracked_files.push(BaselineFile {
                    path: path.into(),
                    hash: String::new(),
                    blob_id: None,
                    excluded_reason: Some(error.to_string()),
                }),
            }
        }
        Some(GitBaseline {
            repository: true,
            branch: (!branch.is_empty()).then_some(branch),
            head: (!head.is_empty()).then_some(head),
            status,
            staged_diff_blob: (!staged.is_empty())
                .then(|| store.blob(staged.as_bytes()))
                .transpose()?,
            unstaged_diff_blob: (!unstaged.is_empty())
                .then(|| store.blob(unstaged.as_bytes()))
                .transpose()?,
            untracked_files,
        })
    } else {
        Some(GitBaseline {
            repository: false,
            ..GitBaseline::default()
        })
    };
    let manifest = store
        .get::<Value>("repo_map", &workspace.id)
        .unwrap_or_else(|_| json!({"workspace":workspace.id,"root":workspace.root}));
    let events = store.events(&session.id, None, 0)?;
    let baseline = ImplementationBaseline {
        id: crate::id(),
        workspace_id: workspace.id.clone(),
        session_id: session.id.clone(),
        plan_id: plan.id.clone(),
        plan_revision: plan.revision,
        run_id: run_id.into(),
        git,
        workspace_manifest_hash: crate::hash(&serde_json::to_vec(&manifest)?),
        context_revision,
        session_sequence: events
            .last()
            .map(|event| event.session_sequence)
            .unwrap_or(0),
        created_at: crate::now(),
    };
    store.put("implementation_baseline", &baseline.id, &baseline)?;
    Ok(baseline)
}

pub fn latest_checkpoint(store: &Store, plan_id: &str) -> Result<Option<ImplementationCheckpoint>> {
    let mut items: Vec<ImplementationCheckpoint> = store
        .list("implementation_checkpoint")?
        .into_iter()
        .filter(|item: &ImplementationCheckpoint| item.plan_id == plan_id)
        .collect();
    items.sort_by(|a, b| b.revision.cmp(&a.revision));
    Ok(items.into_iter().next())
}

fn evidence(
    store: &Store,
    session_id: &str,
    run_id: &str,
    after: i64,
) -> Result<(
    Vec<ChangedFileEvidence>,
    Vec<ValidationEvidence>,
    i64,
    String,
)> {
    let events: Vec<_> = store
        .events(session_id, None, after)?
        .into_iter()
        .filter(|event| event.run_id == run_id || event.session_id == session_id)
        .collect();
    let last = events
        .last()
        .map(|event| event.session_sequence)
        .unwrap_or(after);
    let hash = crate::hash(&serde_json::to_vec(&events)?);
    let mut changed = Vec::new();
    for checkpoint in store.list::<Value>("checkpoint")? {
        if checkpoint["run_id"] == run_id && checkpoint["state"] == "applied" {
            changed.push(ChangedFileEvidence {
                path: checkpoint["path"].as_str().unwrap_or_default().into(),
                before_hash: checkpoint["before_hash"].as_str().map(str::to_owned),
                after_hash: checkpoint["after_hash"].as_str().unwrap_or_default().into(),
                file_checkpoint_ids: vec![checkpoint["id"].as_str().unwrap_or_default().into()],
                summary: "Alteração aplicada com checkpoint de arquivo".into(),
            });
        }
    }
    let mut validations = Vec::new();
    for event in &events {
        if event.r#type == "tool.completed" && event.payload["name"] == "terminal.exec" {
            let result = &event.payload["output"];
            validations.push(ValidationEvidence {
                command: None,
                description: "Comando executado pelo agente".into(),
                state: if result["success"] == true {
                    "passed"
                } else {
                    "failed"
                }
                .into(),
                exit_code: result.pointer("/result/exit_code").and_then(Value::as_i64),
                event_id: Some(event.event_id.clone()),
                artifact_id: None,
            });
        }
    }
    Ok((changed, validations, last, hash))
}

pub fn checkpoint(
    store: &Store,
    workspace: &Workspace,
    plan: &PlanArtifact,
    run_id: &str,
    reason: &str,
    status: &str,
    current_step: Option<&str>,
    note: Option<&str>,
) -> Result<ImplementationCheckpoint> {
    let previous = latest_checkpoint(store, &plan.id)?;
    let revision = previous
        .as_ref()
        .map(|value| value.revision + 1)
        .unwrap_or(1);
    let first = previous
        .as_ref()
        .map(|value| value.last_session_sequence + 1)
        .unwrap_or(0);
    let (changed, validations, last, source_events_hash) =
        evidence(store, &plan.session_id, run_id, first.saturating_sub(1))?;
    let mut steps: Vec<ProgressStep> = previous
        .as_ref()
        .map(|value| value.steps.clone())
        .unwrap_or_else(|| {
            plan.steps
                .iter()
                .map(|step| ProgressStep {
                    id: step.id.clone(),
                    state: if step.dependencies.is_empty() {
                        "ready"
                    } else {
                        "pending"
                    }
                    .into(),
                    started_at: None,
                    completed_at: None,
                    summary: None,
                    evidence_event_ids: vec![],
                })
                .collect()
        });
    if let Some(id) = current_step {
        let entry = steps
            .iter_mut()
            .find(|step| step.id == id)
            .context("Etapa não encontrada")?;
        match reason {
            "step_started" => {
                entry.state = "running".into();
                entry.started_at.get_or_insert_with(crate::now);
            }
            "step_completed" => {
                entry.state = "completed".into();
                entry.completed_at = Some(crate::now());
                entry.summary = note.map(str::to_owned);
            }
            "step_failed" => {
                entry.state = "failed".into();
                entry.summary = note.map(str::to_owned);
            }
            _ => {}
        }
    }
    let states: HashMap<_, _> = steps
        .iter()
        .map(|step| (step.id.clone(), step.state.clone()))
        .collect();
    for (progress, definition) in steps.iter_mut().zip(&plan.steps) {
        if progress.state == "pending"
            && definition
                .dependencies
                .iter()
                .all(|id| states.get(id).is_some_and(|state| state == "completed"))
        {
            progress.state = "ready".into();
        }
    }
    let completed_work = steps
        .iter()
        .filter(|step| step.state == "completed")
        .map(|step| {
            format!(
                "{}: {}",
                step.id,
                step.summary.as_deref().unwrap_or("concluída")
            )
        })
        .collect();
    let pending_work = plan
        .steps
        .iter()
        .filter(|definition| {
            steps
                .iter()
                .find(|step| step.id == definition.id)
                .is_some_and(|step| !matches!(step.state.as_str(), "completed" | "skipped"))
        })
        .map(|step| format!("{}: {}", step.id, step.title))
        .collect();
    let agent_task_ids = store
        .list::<AgentTask>("agent_task")?
        .into_iter()
        .filter(|task| {
            task.session_id == plan.session_id
                && task.plan_id.as_deref().is_none_or(|id| id == plan.id)
        })
        .map(|task| task.id)
        .collect();
    let browser_artifact_ids = store
        .list::<Artifact>("artifact")?
        .into_iter()
        .filter(|artifact| {
            artifact.session_id == plan.session_id && artifact.kind == "browser_screenshot"
        })
        .map(|artifact| artifact.id)
        .collect();
    let item = ImplementationCheckpoint {
        id: crate::id(),
        workspace_id: workspace.id.clone(),
        session_id: plan.session_id.clone(),
        plan_id: plan.id.clone(),
        plan_revision: plan.revision,
        root_run_id: run_id.into(),
        revision,
        reason: reason.into(),
        status: status.into(),
        current_step_id: current_step.map(str::to_owned),
        steps,
        completed_work,
        pending_work,
        decisions: previous
            .as_ref()
            .map(|value| value.decisions.clone())
            .unwrap_or_default(),
        blockers: if status == "blocked" {
            note.map(|value| vec![value.into()]).unwrap_or_default()
        } else {
            vec![]
        },
        unresolved_risks: plan.risks.clone(),
        next_actions: note.map(|value| vec![value.into()]).unwrap_or_default(),
        changed_files: merge_files(
            previous
                .as_ref()
                .map(|value| value.changed_files.as_slice())
                .unwrap_or_default(),
            &changed,
        ),
        validations: merge_validations(
            previous
                .as_ref()
                .map(|value| value.validations.as_slice())
                .unwrap_or_default(),
            &validations,
        ),
        active_agent_task_ids: agent_task_ids,
        browser_artifact_ids,
        first_session_sequence: first,
        last_session_sequence: last,
        source_events_hash,
        previous_checkpoint_id: previous.as_ref().map(|value| value.id.clone()),
        created_at: crate::now(),
    };
    store.put("implementation_checkpoint", &item.id, &item)?;
    materialize_progress(workspace, plan, &item)?;
    Ok(item)
}

fn merge_files(
    old: &[ChangedFileEvidence],
    new: &[ChangedFileEvidence],
) -> Vec<ChangedFileEvidence> {
    let mut map: HashMap<String, ChangedFileEvidence> = old
        .iter()
        .cloned()
        .map(|value| (value.path.clone(), value))
        .collect();
    for value in new {
        map.entry(value.path.clone())
            .and_modify(|old| {
                old.after_hash = value.after_hash.clone();
                for id in &value.file_checkpoint_ids {
                    if !old.file_checkpoint_ids.contains(id) {
                        old.file_checkpoint_ids.push(id.clone());
                    }
                }
            })
            .or_insert_with(|| value.clone());
    }
    let mut values: Vec<_> = map.into_values().collect();
    values.sort_by(|a, b| a.path.cmp(&b.path));
    values
}

fn merge_validations(
    old: &[ValidationEvidence],
    new: &[ValidationEvidence],
) -> Vec<ValidationEvidence> {
    let mut values = old.to_vec();
    for item in new {
        if !values.iter().any(|old| old.event_id == item.event_id) {
            values.push(item.clone());
        }
    }
    values
}

fn materialize_progress(
    workspace: &Workspace,
    plan: &PlanArtifact,
    cp: &ImplementationCheckpoint,
) -> Result<()> {
    let plan_name = Path::new(&plan.markdown_path)
        .file_stem()
        .context("Plano sem nome")?
        .to_string_lossy();
    let target = plan_dir(Path::new(&workspace.root))?.join(format!("{plan_name}.progress.md"));
    let checklist: String = plan
        .steps
        .iter()
        .map(|definition| {
            let state = cp
                .steps
                .iter()
                .find(|step| step.id == definition.id)
                .map(|step| step.state.as_str())
                .unwrap_or("pending");
            format!(
                "- [{}] **{} — {}** · {}\n",
                if state == "completed" { "x" } else { " " },
                definition.id,
                definition.title,
                state
            )
        })
        .collect();
    let files: String = if cp.changed_files.is_empty() {
        "- Nenhum arquivo alterado.\n".into()
    } else {
        cp.changed_files
            .iter()
            .map(|file| {
                format!(
                    "- `{}` — {} (`{}`)\n",
                    file.path, file.summary, file.after_hash
                )
            })
            .collect()
    };
    let validations: String = if cp.validations.is_empty() {
        "- Nenhuma validação registrada.\n".into()
    } else {
        cp.validations
            .iter()
            .map(|item| format!("- [{}] {}\n", item.state, item.description))
            .collect()
    };
    let blockers: String = if cp.blockers.is_empty() {
        "- Nenhum bloqueio.\n".into()
    } else {
        cp.blockers
            .iter()
            .map(|value| format!("- {value}\n"))
            .collect()
    };
    let next_actions: String = if cp.next_actions.is_empty() {
        "- Continuar a próxima etapa pronta.\n".into()
    } else {
        cp.next_actions
            .iter()
            .map(|value| format!("- {value}\n"))
            .collect()
    };
    let body = format!("# Progresso da implementação\n\nPlano: **{}**  \nRevisão do plano: {}  \nCheckpoint: {}  \nEstado: **{}**  \nAtualizado: {}\n\n## Etapas\n\n{}\n## Arquivos alterados\n\n{}\n## Validações\n\n{}\n## Bloqueios e riscos\n\n{}\n## Próximos passos\n\n{}", plan.title, plan.revision, cp.revision, cp.status, cp.created_at, checklist, files, validations, blockers, next_actions);
    files::atomic(&target, body.as_bytes())
}

pub fn pinned_context(store: &Store, session_id: &str) -> Result<Option<Value>> {
    let Some(plan) = active(store, session_id)? else {
        return Ok(None);
    };
    let checkpoint = latest_checkpoint(store, &plan.id)?;
    Ok(Some(
        json!({"active_plan":{"id":plan.id,"revision":plan.revision,"source_hash":plan.source_hash,"objective":plan.objective,"constraints":plan.constraints,"decisions":plan.decisions,"acceptance_criteria":plan.acceptance_criteria,"steps":plan.steps.iter().map(|step|json!({"id":step.id,"title":step.title,"dependencies":step.dependencies,"state":checkpoint.as_ref().and_then(|cp|cp.steps.iter().find(|candidate|candidate.id==step.id)).map(|step|step.state.as_str()).unwrap_or("pending")})).collect::<Vec<_>>()},"latest_checkpoint":checkpoint}),
    ))
}

pub fn update_progress(
    store: &Store,
    workspace: &Workspace,
    plan: &PlanArtifact,
    run: &Run,
    input: &Value,
) -> Result<ImplementationCheckpoint> {
    ensure!(
        input["plan_id"] == plan.id && input["plan_revision"].as_u64() == Some(plan.revision),
        "Plano ou revisão divergente"
    );
    let step_id = input["step_id"].as_str().context("Etapa obrigatória")?;
    let state = input["state"].as_str().context("Estado obrigatório")?;
    ensure!(
        ["running", "blocked", "failed", "completed", "skipped"].contains(&state),
        "Estado de progresso inválido"
    );
    let previous = latest_checkpoint(store, &plan.id)?.context("Checkpoint inicial ausente")?;
    let definition = plan
        .steps
        .iter()
        .find(|step| step.id == step_id)
        .context("Etapa inexistente")?;
    if state == "completed" {
        for dependency in &definition.dependencies {
            ensure!(
                previous
                    .steps
                    .iter()
                    .any(|step| step.id == *dependency && step.state == "completed"),
                "Conclua as dependências antes desta etapa"
            );
        }
    }
    let evidence_ids: Vec<String> = input
        .get("evidence_event_ids")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let events = store.events(&plan.session_id, None, 0)?;
    ensure!(
        evidence_ids
            .iter()
            .all(|id| events.iter().any(|event| &event.event_id == id)),
        "Evidência inexistente ou de outra conversa"
    );
    let reason = match state {
        "running" => "step_started",
        "completed" => "step_completed",
        "failed" | "blocked" => "step_failed",
        _ => "manual",
    };
    let status = match state {
        "blocked" => "blocked",
        "failed" => "failed",
        _ => "running",
    };
    checkpoint(
        store,
        workspace,
        plan,
        &run.id,
        reason,
        status,
        Some(step_id),
        input["summary"].as_str(),
    )
}

pub fn list_for_session(store: &Store, session_id: &str) -> Result<Vec<PlanArtifact>> {
    let mut values: Vec<_> = store
        .list::<PlanArtifact>("plan")?
        .into_iter()
        .filter(|plan| plan.session_id == session_id)
        .collect();
    values.sort_by(|a, b| b.revision.cmp(&a.revision));
    Ok(values)
}

fn validated_draft(input: &Value) -> Result<(DraftPlan, Vec<PlanStep>)> {
    let draft: DraftPlan = serde_json::from_value(input.clone())?;
    ensure!(
        !draft.acceptance_criteria.is_empty() && draft.acceptance_criteria.len() <= 100,
        "Informe critérios de aceite"
    );
    let steps: Vec<_> = draft
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            Ok(PlanStep {
                id: text(&step.id, "Identificador", 80)?,
                order: index as u32 + 1,
                title: text(&step.title, "Título da etapa", 300)?,
                description: text(&step.description, "Descrição da etapa", 5_000)?,
                dependencies: step.dependencies.clone(),
                expected_files: step.expected_files.clone(),
                validation: step.validation.clone(),
            })
        })
        .collect::<Result<_>>()?;
    validate_graph(&steps)?;
    Ok((draft, steps))
}

pub fn propose_revision(
    store: &Store,
    workspace: &Workspace,
    plan: &mut PlanArtifact,
    run: &Run,
    input: &Value,
) -> Result<PlanRevisionProposal> {
    ensure!(
        plan.state == "implementing",
        "Somente um plano em implementação pode receber revisão"
    );
    let draft_value = input
        .get("plan")
        .context("Plano revisado obrigatório")?
        .clone();
    let (_, steps) = validated_draft(&draft_value)?;
    let changed_sections: Vec<String> = input
        .get("changed_sections")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_else(|| vec!["plano".into()]);
    let preview = json!({"title":draft_value["title"],"summary":draft_value["summary"],"objective":draft_value["objective"],"constraints":draft_value["constraints"],"decisions":draft_value["decisions"],"acceptance_criteria":draft_value["acceptance_criteria"],"steps":steps,"risks":draft_value["risks"]});
    let before = serde_json::to_string_pretty(
        &json!({"title":plan.title,"summary":plan.summary,"objective":plan.objective,"constraints":plan.constraints,"decisions":plan.decisions,"acceptance_criteria":plan.acceptance_criteria,"steps":plan.steps,"risks":plan.risks}),
    )?;
    let after = serde_json::to_string_pretty(&preview)?;
    let markdown_diff = similar::TextDiff::from_lines(&before, &after)
        .unified_diff()
        .header(
            &format!("plano-r{}", plan.revision),
            &format!("plano-r{}", plan.revision + 1),
        )
        .to_string();
    let proposal = PlanRevisionProposal {
        id: crate::id(),
        plan_id: plan.id.clone(),
        from_revision: plan.revision,
        proposed_revision: plan.revision + 1,
        reason: text(input["reason"].as_str().unwrap_or(""), "Motivo", 5_000)?,
        changed_sections,
        markdown_diff,
        state: "pending".into(),
        created_by_run_id: run.id.clone(),
        created_at: crate::now(),
    };
    store.put("plan_revision_draft", &proposal.id, &draft_value)?;
    store.put("plan_revision_proposal", &proposal.id, &proposal)?;
    plan.state = "revision_pending".into();
    plan.updated_at = crate::now();
    store.put("plan", &plan.id, plan)?;
    checkpoint(
        store,
        workspace,
        plan,
        &run.id,
        "before_pause",
        "paused",
        None,
        Some("Revisão do plano aguardando aprovação"),
    )?;
    Ok(proposal)
}

pub fn approve_revision(
    store: &Store,
    workspace: &Workspace,
    proposal_id: &str,
) -> Result<(PlanRevisionProposal, PlanArtifact)> {
    let mut proposal: PlanRevisionProposal = store.get("plan_revision_proposal", proposal_id)?;
    ensure!(proposal.state == "pending", "A proposta já foi decidida");
    let previous: PlanArtifact = store.get("plan", &proposal.plan_id)?;
    let session: Session = store.get("session", &previous.session_id)?;
    let source: Run = store.get("run", &proposal.created_by_run_id)?;
    let mut planning_run = source.clone();
    planning_run.mode = Some(Mode::Plan);
    let draft: Value = store.get("plan_revision_draft", proposal_id)?;
    let mut next = create(store, workspace, &session, &planning_run, &draft)?;
    next.state = if previous.implementation_run_id.is_some() {
        "implementing"
    } else {
        "ready"
    }
    .into();
    next.implementation_run_id = previous.implementation_run_id.clone();
    next.baseline_id = previous.baseline_id.clone();
    next.updated_at = crate::now();
    store.put("plan", &next.id, &next)?;
    if let Some(old) = latest_checkpoint(store, &previous.id)? {
        let mut progress: Vec<ProgressStep> = next
            .steps
            .iter()
            .map(|definition| {
                old.steps
                    .iter()
                    .find(|step| step.id == definition.id)
                    .cloned()
                    .unwrap_or(ProgressStep {
                        id: definition.id.clone(),
                        state: if definition.dependencies.is_empty() {
                            "ready"
                        } else {
                            "pending"
                        }
                        .into(),
                        started_at: None,
                        completed_at: None,
                        summary: None,
                        evidence_event_ids: vec![],
                    })
            })
            .collect();
        let states: HashMap<_, _> = progress
            .iter()
            .map(|step| (step.id.clone(), step.state.clone()))
            .collect();
        for (item, definition) in progress.iter_mut().zip(&next.steps) {
            if item.state == "pending"
                && definition
                    .dependencies
                    .iter()
                    .all(|id| states.get(id).is_some_and(|state| state == "completed"))
            {
                item.state = "ready".into()
            }
        }
        let cp = ImplementationCheckpoint {
            id: crate::id(),
            workspace_id: workspace.id.clone(),
            session_id: next.session_id.clone(),
            plan_id: next.id.clone(),
            plan_revision: next.revision,
            root_run_id: next.implementation_run_id.clone().unwrap_or(source.id),
            revision: 1,
            reason: "recovered".into(),
            status: "running".into(),
            current_step_id: old
                .current_step_id
                .filter(|id| next.steps.iter().any(|step| step.id == *id)),
            steps: progress,
            completed_work: old.completed_work,
            pending_work: next
                .steps
                .iter()
                .filter(|definition| {
                    !old.steps
                        .iter()
                        .any(|step| step.id == definition.id && step.state == "completed")
                })
                .map(|step| format!("{}: {}", step.id, step.title))
                .collect(),
            decisions: old.decisions,
            blockers: vec![],
            unresolved_risks: next.risks.clone(),
            next_actions: vec!["Continuar a partir da revisão aprovada".into()],
            changed_files: old.changed_files,
            validations: old.validations,
            active_agent_task_ids: old.active_agent_task_ids,
            browser_artifact_ids: old.browser_artifact_ids,
            first_session_sequence: old.last_session_sequence + 1,
            last_session_sequence: old.last_session_sequence,
            source_events_hash: crate::hash(b"revision-approved"),
            previous_checkpoint_id: Some(old.id),
            created_at: crate::now(),
        };
        store.put("implementation_checkpoint", &cp.id, &cp)?;
        materialize_progress(workspace, &next, &cp)?;
    }
    proposal.state = "approved".into();
    store.put("plan_revision_proposal", proposal_id, &proposal)?;
    Ok((proposal, next))
}

pub fn reject_revision(
    store: &Store,
    proposal_id: &str,
) -> Result<(PlanRevisionProposal, PlanArtifact)> {
    let mut proposal: PlanRevisionProposal = store.get("plan_revision_proposal", proposal_id)?;
    ensure!(proposal.state == "pending", "A proposta já foi decidida");
    let mut plan: PlanArtifact = store.get("plan", &proposal.plan_id)?;
    plan.state = if plan.implementation_run_id.is_some() {
        "implementing"
    } else {
        "ready"
    }
    .into();
    plan.updated_at = crate::now();
    store.put("plan", &plan.id, &plan)?;
    proposal.state = "rejected".into();
    store.put("plan_revision_proposal", proposal_id, &proposal)?;
    Ok((proposal, plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_cycles() {
        let steps = vec![
            PlanStep {
                id: "a".into(),
                order: 1,
                title: "A".into(),
                description: "A".into(),
                dependencies: vec!["b".into()],
                expected_files: vec![],
                validation: vec![],
            },
            PlanStep {
                id: "b".into(),
                order: 2,
                title: "B".into(),
                description: "B".into(),
                dependencies: vec!["a".into()],
                expected_files: vec![],
                validation: vec![],
            },
        ];
        assert!(validate_graph(&steps).is_err());
    }

    #[tokio::test]
    async fn persists_plan_baseline_checkpoint_chain_and_pinned_context() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("app.ts"), "export const ready = true;\n").unwrap();
        let store = Store::open(data.path()).unwrap();
        let workspace = Workspace {
            id: "workspace".into(),
            name: "Projeto".into(),
            root: root.path().to_string_lossy().into_owned(),
            created_at: crate::now(),
        };
        let session = Session {
            id: "session".into(),
            workspace_id: workspace.id.clone(),
            title: "Plano".into(),
            mode: Mode::Plan,
            provider_id: "provider".into(),
            model: "model".into(),
            created_at: crate::now(),
            updated_at: crate::now(),
            executor_profile_id: None,
            reviewer_profile_ids: vec![],
            reasoning_level: None,
            last_run_id: None,
            archived: false,
        };
        let run = Run {
            id: "planning-run".into(),
            session_id: session.id.clone(),
            goal: "Planejar".into(),
            state: "completed".into(),
            created_at: crate::now(),
            max_turns: 10,
            selected_skills: vec![],
            mode: Some(Mode::Plan),
            executor_profile_id: None,
            reviewer_profile_ids: vec![],
            reasoning_level: None,
            context_revision: 0,
            resumed_from_run_id: None,
        };
        store.put("workspace", &workspace.id, &workspace).unwrap();
        store.put("session", &session.id, &session).unwrap();
        store.put("run", &run.id, &run).unwrap();
        let plan = create(
            &store,
            &workspace,
            &session,
            &run,
            &json!({
                "title":"Corrigir continuidade",
                "summary":"Plano canônico persistente",
                "objective":"Manter o estado depois da compactação",
                "constraints":["Não apagar o histórico"],
                "decisions":["Usar checkpoints imutáveis"],
                "acceptance_criteria":["Plano e progresso recuperáveis"],
                "steps":[
                    {"id":"P1","title":"Persistir","description":"Salvar o plano","validation":["Documento existe"]},
                    {"id":"P2","title":"Retomar","description":"Carregar o checkpoint","dependencies":["P1"],"validation":["Contexto fixado existe"]}
                ]
            }),
        )
        .unwrap();
        let markdown_path = root.path().join(&plan.markdown_path);
        let markdown = std::fs::read(&markdown_path).unwrap();
        assert_eq!(
            store.read_blob(&plan.markdown_artifact_id).unwrap(),
            markdown
        );

        let baseline = baseline(&store, &workspace, &session, &plan, "implementation-run", 3)
            .await
            .unwrap();
        assert_eq!(baseline.plan_id, plan.id);
        assert!(!baseline.workspace_manifest_hash.is_empty());
        let first = checkpoint(
            &store,
            &workspace,
            &plan,
            "implementation-run",
            "implementation_started",
            "running",
            None,
            Some("Começar P1"),
        )
        .unwrap();
        let second = checkpoint(
            &store,
            &workspace,
            &plan,
            "implementation-run",
            "step_completed",
            "running",
            Some("P1"),
            Some("Plano salvo"),
        )
        .unwrap();
        assert_eq!(
            second.previous_checkpoint_id.as_deref(),
            Some(first.id.as_str())
        );
        assert_eq!(second.steps[0].state, "completed");
        assert_eq!(second.steps[1].state, "ready");
        assert!(
            root.path()
                .join("docs/forja-plans")
                .read_dir()
                .unwrap()
                .count()
                >= 2
        );
        let pinned = pinned_context(&store, &session.id).unwrap().unwrap();
        assert_eq!(pinned["active_plan"]["id"], plan.id);
        assert_eq!(pinned["latest_checkpoint"]["revision"], 2);
    }
}
