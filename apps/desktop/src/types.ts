export type Mode='consult'|'plan'|'agent'|'review';
export interface Workspace{id:string;name:string;root:string;created_at:string}
export interface Provider{id:string;name:string;kind:string;base_url:string;local_only:boolean;secret_ref?:string}
export interface SearchProvider{id:string;kind:'brave'|'searxng'|'declarative';name:string;base_url:string;secret_ref?:string;enabled:boolean;allow_local:boolean;revision:number}
export interface Session{id:string;workspace_id:string;title:string;mode:Mode;provider_id:string;model:string;created_at:string;updated_at?:string;executor_profile_id?:string;reviewer_profile_ids?:string[];reasoning_level?:string;last_run_id?:string;archived?:boolean}
export interface Run{id:string;session_id:string;goal:string;state:string;created_at:string;selected_skills?:{path:string;hash:string}[];mode?:Mode;executor_profile_id?:string;reviewer_profile_ids?:string[];reasoning_level?:string;context_revision?:number;resumed_from_run_id?:string}
export interface Event{event_id:string;session_id:string;run_id:string;sequence:number;session_sequence:number;timestamp:string;type:string;payload:Record<string,any>}
export interface FileEntry{path:string;name:string;directory:boolean;size:number}
export interface FileContent{path:string;content:string;hash:string}
export interface Approval{id:string;session_id:string;run_id:string;tool:{name:string;arguments:Record<string,any>};state:string}
export type MetadataSource='provider'|'probe'|'manual'|'unknown';
export interface CapabilitySupport{state:'supported'|'unsupported'|'unknown';source:MetadataSource}
export interface ModelProfile{id:string;provider_id:string;model_id:string;display_name:string;enabled:boolean;revision:number;context_window_tokens?:number;context_source:MetadataSource;max_output_tokens?:number;max_output_source:MetadataSource;capabilities:{text:CapabilitySupport;vision:CapabilitySupport;tools:CapabilitySupport;structured_output:CapabilitySupport;reasoning:CapabilitySupport};reasoning_levels:string[];default_reasoning_level?:string;created_at:string;updated_at:string}
export interface ContextState{session_id:string;model_profile_id:string;context_revision:number;context_window_tokens:number;context_limit_source:MetadataSource;reserved_output_tokens:number;safety_margin_tokens:number;usable_input_tokens:number;used_input_tokens:number;usage_percent:number;count_source:'provider_exact'|'usage_actual'|'local_estimate';breakdown:{system:number;tools:number;history:number;attachments:number;current_input:number;plan?:number;implementation_checkpoint?:number};compacted_through_sequence?:number;summary_id?:string;updated_at:string}
export interface InteractionOption{id:string;label:string;description?:string}
export interface Interaction{id:string;session_id:string;run_id:string;question:string;detail?:string;kind:'single'|'multiple'|'text';options:InteractionOption[];required:boolean;allow_custom:boolean;state:'pending'|'answered'|'cancelled'|'expired';answer?:unknown;created_at:string;updated_at:string}
export interface PlanStep{id:string;order:number;title:string;description:string;dependencies:string[];expected_files:string[];validation:string[]}
export interface PlanArtifact{id:string;workspace_id:string;session_id:string;source_run_id:string;revision:number;state:'ready'|'accepted'|'implementing'|'implemented'|'implementation_failed'|'revision_pending'|'superseded';title:string;summary:string;objective:string;constraints:string[];decisions:string[];acceptance_criteria:string[];steps:PlanStep[];risks:string[];source_hash:string;markdown_artifact_id:string;markdown_path:string;implementation_run_id?:string;baseline_id?:string;created_at:string;updated_at:string}
export interface ProgressStep{id:string;state:'pending'|'ready'|'running'|'blocked'|'failed'|'completed'|'skipped';started_at?:string;completed_at?:string;summary?:string;evidence_event_ids:string[]}
export interface ImplementationCheckpoint{id:string;workspace_id:string;session_id:string;plan_id:string;plan_revision:number;root_run_id:string;revision:number;reason:string;status:string;current_step_id?:string;steps:ProgressStep[];completed_work:string[];pending_work:string[];decisions:string[];blockers:string[];unresolved_risks:string[];next_actions:string[];changed_files:{path:string;after_hash:string;summary:string}[];validations:{description:string;state:string;command?:string}[];created_at:string}
export interface CodeProposal{id:string;workspace_id:string;session_id:string;run_id:string;path:string;language?:string;base_hash:string;old_text:string;new_text:string;explanation?:string;state:'pending'|'applied'|'conflict'|'dismissed';checkpoint_id?:string;created_at:string;updated_at:string}
export interface AgentProfile{id:string;workspace_id:string;name:string;role:string;instructions:string;model_profile_id:string;reasoning_level?:string;allowed_tools:string[];write_access:'none'|'workspace'|'worktree';max_turns:number;token_budget?:number;time_budget_seconds:number;created_by:'user'|'agent';enabled:boolean;revision:number;created_at:string;updated_at:string}
export interface PlanRevisionProposal{id:string;plan_id:string;from_revision:number;proposed_revision:number;reason:string;changed_sections:string[];markdown_diff:string;state:'pending'|'approved'|'rejected';created_by_run_id:string;created_at:string}
export interface ChatItem{id:string;kind:'user'|'assistant'|'tool'|'status';text:string;event?:Event}
export function timeline(events:Event[]):ChatItem[]{
 const out:ChatItem[]=[];let current:ChatItem|undefined;const calls=new Map<string,Record<string,any>>();
 for(const e of events){
  if(e.type==='run.started')out.push({id:e.event_id,kind:'user',text:e.payload.goal});
  else if(e.type==='message.started'){current={id:e.event_id,kind:'assistant',text:''};out.push(current);}
  else if(e.type==='message.delta'&&current)current.text+=e.payload.text??'';
  else if(e.type==='message.completed'){if(current)current.text=e.payload.text??current.text;current=undefined;}
  else if(e.type==='tool.started')calls.set(String(e.payload.id),e.payload.arguments??{});
  else if(e.type==='tool.completed')out.push({id:e.event_id,kind:'tool',text:e.payload.name,event:{...e,payload:{...e.payload,arguments:calls.get(String(e.payload.id))??{}}}});
  else if(e.type==='hook.completed')out.push({id:e.event_id,kind:'tool',text:'Hook: '+e.payload.name,event:e});
  else if(e.type==='approval.revoked')out.push({id:e.event_id,kind:'status',text:e.payload.message,event:e});
  else if(e.type==='skills.selected')out.push({id:e.event_id,kind:'status',text:'Skills escolhidas: '+(e.payload.skills??[]).map((s:any)=>s.metadata?.name??s.source).join(', '),event:e});
  else if(e.type==='skill.loaded')out.push({id:e.event_id,kind:'status',text:'Skill carregada: '+e.payload.source,event:e});
  else if(e.type==='context.compacted')out.push({id:e.event_id,kind:'status',text:'Contexto compactado com segurança.',event:e});
  else if(e.type==='context.compaction.failed')out.push({id:e.event_id,kind:'status',text:e.payload.message??'Não foi possível compactar o contexto.',event:e});
  else if(e.type==='reviewer.completed')out.push({id:e.event_id,kind:'tool',text:'Revisão: '+(e.payload.model??'modelo'),event:e});
  else if(e.type==='plan.ready')out.push({id:e.event_id,kind:'status',text:'Plano estruturado salvo em '+(e.payload.markdown_path??'docs/forja-plans'),event:e});
  else if(e.type==='plan.implementation.started')out.push({id:e.event_id,kind:'status',text:'Implementação iniciada a partir do baseline persistente.',event:e});
  else if(e.type==='implementation.checkpoint.created')out.push({id:e.event_id,kind:'status',text:'Checkpoint de implementação '+(e.payload.revision??'')+' salvo.',event:e});
  else if(e.type==='code.proposal.created')out.push({id:e.event_id,kind:'status',text:'Proposta de código pronta para revisão.',event:e});
  else if(e.type==='browser.screenshot')out.push({id:e.event_id,kind:'tool',text:'browser.screenshot',event:e});
  else if(['run.failed','run.cancelled','run.interrupted'].includes(e.type))out.push({id:e.event_id,kind:'status',text:e.payload.message,event:e});
 }return out;
}
