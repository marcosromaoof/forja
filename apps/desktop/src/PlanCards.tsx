import {Check,ChevronRight,FileText,Hammer,History,Play,RefreshCw,Shield,TerminalSquare,X} from 'lucide-react';
import type {CodeProposal,ImplementationCheckpoint,PlanArtifact,PlanRevisionProposal} from './types';

export function PlanCard({plan,checkpoint,busy,onImplement,onOpen,onCheckpoint}:{plan:PlanArtifact;checkpoint:ImplementationCheckpoint|null;busy:boolean;onImplement:()=>void;onOpen:()=>void;onCheckpoint:()=>void}){
 const completed=checkpoint?.steps.filter(step=>step.state==='completed').length??0;
 const current=checkpoint?.steps.find(step=>step.state==='running');
 const canImplement=['ready','implementation_failed','accepted'].includes(plan.state);
 return <article className="plan-card" aria-label={'Plano '+plan.title}>
  <header><span><FileText size={17}/>PLANO PERSISTENTE</span><b>R{plan.revision}</b></header>
  <h3>{plan.title}</h3><p>{plan.summary}</p>
  <div className="plan-stats"><span>{completed}/{plan.steps.length} etapas</span><span>{plan.acceptance_criteria.length} critérios</span><span className={'plan-state '+plan.state}>{labelState(plan.state)}</span></div>
  {checkpoint&&<div className="plan-progress"><i style={{width:`${Math.round(completed/Math.max(1,plan.steps.length)*100)}%`}}/><span>{current?'Em execução: '+current.id:'Checkpoint '+checkpoint.revision}</span></div>}
  <ol>{plan.steps.slice(0,6).map(step=><li key={step.id} className={checkpoint?.steps.find(item=>item.id===step.id)?.state}><span>{checkpoint?.steps.find(item=>item.id===step.id)?.state==='completed'?<Check size={12}/>:<ChevronRight size={12}/>}</span><strong>{step.id}</strong>{step.title}</li>)}</ol>
  <small><Shield size={12}/>{plan.markdown_path}</small>
  <footer><button className="subtle-button" onClick={onOpen}><FileText size={14}/>Abrir plano</button>{plan.state==='implementing'&&<button className="subtle-button" onClick={onCheckpoint}><History size={14}/>Criar checkpoint</button>}{canImplement&&<button className="primary-button" disabled={busy} onClick={onImplement}>{plan.state==='implementation_failed'?<RefreshCw size={14}/>:<Hammer size={14}/>} {plan.state==='implementation_failed'?'Retomar implementação':'Implementar Plano'}</button>}</footer>
 </article>
}

function labelState(state:PlanArtifact['state']){return ({ready:'Pronto',accepted:'Aceito',implementing:'Em execução',implemented:'Concluído',implementation_failed:'Interrompido',revision_pending:'Revisão pendente',superseded:'Substituído'} as const)[state]}

export function CodeProposalCard({proposal,busy,onApply,onDismiss,onOpen}:{proposal:CodeProposal;busy:boolean;onApply:()=>void;onDismiss:()=>void;onOpen:()=>void}){
 const oldLines=proposal.old_text.split('\n').slice(0,80),newLines=proposal.new_text.split('\n').slice(0,80);
 return <article className={'code-proposal '+proposal.state}>
  <header><span><TerminalSquare size={16}/>PROPOSTA DE CÓDIGO</span><b>{proposal.state}</b></header>
  <button className="proposal-path" onClick={onOpen}>{proposal.path}</button>
  {proposal.explanation&&<p>{proposal.explanation}</p>}
  <div className="proposal-diff" aria-label="Comparação da proposta"><pre>{oldLines.map((line,index)=><span className="remove" key={'o'+index}>- {line}</span>)}</pre><pre>{newLines.map((line,index)=><span className="add" key={'n'+index}>+ {line}</span>)}</pre></div>
  <small>Hash-base <code>{proposal.base_hash.slice(0,12)}</code> · aplicação cria checkpoint</small>
  {['pending','conflict'].includes(proposal.state)&&<footer><button className="subtle-button" onClick={onDismiss}><X size={14}/>Descartar</button><button className="primary-button" disabled={busy||proposal.state==='conflict'} onClick={onApply}><Play size={14}/>Aplicar ao arquivo</button></footer>}
 </article>
}

export function PlanRevisionCard({proposal,busy,onApprove,onReject}:{proposal:PlanRevisionProposal;busy:boolean;onApprove:()=>void;onReject:()=>void}){
 return <article className="revision-card"><header><span>REVISÃO DO PLANO NECESSÁRIA</span><b>R{proposal.from_revision} → R{proposal.proposed_revision}</b></header><h3>{proposal.reason}</h3><p>Seções: {proposal.changed_sections.join(', ')||'plano completo'}</p><pre>{proposal.markdown_diff}</pre><footer><button className="subtle-button" disabled={busy} onClick={onReject}><X size={14}/>Rejeitar</button><button className="primary-button" disabled={busy} onClick={onApprove}><Check size={14}/>Aprovar revisão</button></footer></article>
}
