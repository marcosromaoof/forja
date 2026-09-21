import {useEffect,useState} from 'react';
import {Plus,Workflow,Trash2,Pencil,ArrowUp,ArrowDown,RefreshCw} from 'lucide-react';
import {api} from './api';
import {Modal} from './ui';
import type {Workspace} from './types';

type HookEvent={id:string;label:string;tool_filter:boolean};
type Hook={id:string;workspace_id:string;name:string;event:string;tools:string[];command:string;cwd:string;timeout_seconds:number;enabled:boolean;position:number;revision:number};
const blank:Hook={id:'',workspace_id:'',name:'',event:'before_tool',tools:['fs.apply_patch'],command:'',cwd:'.',timeout_seconds:60,enabled:true,position:0,revision:1};
export function HooksPanel({workspace,onError}:{workspace:Workspace|null;onError:(e:unknown)=>void}){
 const [hooks,setHooks]=useState<Hook[]>([]),[events,setEvents]=useState<HookEvent[]>([]),[tools,setTools]=useState<{name:string}[]>([]),[draft,setDraft]=useState<Hook|null>(null),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true),[formError,setFormError]=useState('');
 useEffect(()=>{let active=true;setHooks([]);setEvents([]);setDraft(null);setLoading(true);if(!workspace){setLoading(false);return}Promise.all([api<Hook[]>('/v1/workspaces/'+workspace.id+'/hooks'),api<{name:string}[]>('/v1/tools'),api<HookEvent[]>('/v1/hooks/events')]).then(([h,t,e])=>{if(active){setHooks(h);setTools(t);setEvents(e)}}).catch(e=>{if(active)onError(e)}).finally(()=>{if(active)setLoading(false)});return()=>{active=false}},[workspace?.id]);
 if(!workspace)return <div className="empty-state"><Workflow size={36}/><h2>Abra um projeto</h2><p>Configure verificações durante as consultas ao modelo e o uso de ferramentas.</p></div>;
 const path='/v1/workspaces/'+workspace.id+'/hooks';
 const activeCount=hooks.filter(h=>h.enabled).length;
 const selectedEvent=events.find(e=>e.id===draft?.event);
 const label=(id:string)=>events.find(e=>e.id===id)?.label??id;
 const ordered=(items:Hook[])=>[...items].sort((a,b)=>a.position-b.position||a.id.localeCompare(b.id));
 const refresh=async()=>{setHooks(await api<Hook[]>(path));};
 async function mutate(action:()=>Promise<void>){setBusy(true);setFormError('');try{await action()}catch(e){setFormError(e instanceof Error?e.message:String(e));if(!draft)onError(e);try{await refresh()}catch{}}finally{setBusy(false)}}
 async function update(h:Hook){const saved=await api<Hook>(path+'/'+h.id,'PUT',{hook:h,expected_revision:h.revision});setHooks(v=>ordered(v.map(x=>x.id===h.id?saved:x)));}
 async function move(index:number,delta:number){const ids=hooks.map(h=>h.id);[ids[index],ids[index+delta]]=[ids[index+delta],ids[index]];setHooks(await api<Hook[]>(path+'/order','POST',{ids,expected:hooks.map(({id,revision,position})=>({id,revision,position}))}));}

 return <div className="integration-panel">
  <div className="integration-toolbar"><span>Hooks de {workspace.name} · {activeCount} {activeCount===1?'ativo':'ativos'}</span><button className="text-button" disabled={busy||loading} onClick={()=>mutate(refresh)}><RefreshCw size={14}/>Atualizar lista</button><button className="primary-button" disabled={busy||loading||events.length===0||hooks.length>=16} onClick={()=>{setFormError('');setDraft({...blank,workspace_id:workspace.id})}}><Plus size={16}/>Criar hook</button></div>
  <p className="field-hint">Cada comando exige aprovação individual e usa o executor com isolamento reduzido. Hooks funcionam em Construir e Revisão. Uma falha interrompe a execução; alterações anteriores são preservadas. Novos hooks e mudanças de ordem valem para a próxima execução. Editar, desativar ou remover revoga aprovações pendentes; comandos já iniciados são interrompidos pelo botão Parar. A ordem vale entre hooks do mesmo momento.</p>
  {loading?<p role="status">Carregando hooks…</p>:hooks.length===0?<div className="empty-state"><Workflow size={36}/><h2>Verificações no momento certo</h2><p>Por exemplo, execute testes depois de uma alteração aprovada.</p></div>:hooks.map((h,index)=><article className="integration-card" key={h.id}><header><Workflow size={23}/><div><h3>{index+1}. {h.name}</h3><p>{label(h.event)}{h.tools.length>0?' · '+h.tools.join(', '):''}</p></div><span className={'server-state '+(h.enabled?'ready':'')}>{h.enabled?'Ativo · sob aprovação':'Desativado'}</span></header><pre className="review-block">{h.command}</pre><p className="field-hint">Diretório: {h.cwd} · Limite: {h.timeout_seconds} s</p><div className="integration-actions hook-actions">
  <button className="text-button" disabled={busy} onClick={()=>{setFormError('');setDraft({...h,tools:[...h.tools]})}}><Pencil size={15}/>Editar</button>
  <button className="text-button" disabled={busy} onClick={()=>mutate(()=>update({...h,enabled:!h.enabled}))}>{h.enabled?'Desativar':'Ativar'}</button>
  <button className="text-button" aria-label={'Mover '+h.name+' para cima'} disabled={busy||index===0} onClick={()=>mutate(()=>move(index,-1))}><ArrowUp size={15}/>Subir</button>
  <button className="text-button" aria-label={'Mover '+h.name+' para baixo'} disabled={busy||index===hooks.length-1} onClick={()=>mutate(()=>move(index,1))}><ArrowDown size={15}/>Descer</button>
  <button className="text-button" disabled={busy} onClick={()=>mutate(async()=>{await api(path+'/'+h.id,'DELETE',{expected_revision:h.revision});setHooks(v=>v.filter(x=>x.id!==h.id))})}><Trash2 size={15}/>Remover hook</button>
 </div></article>)}
  {draft&&<Modal title={draft.id?'Editar hook':'Criar hook'} onClose={()=>{if(!busy)setDraft(null)}}><form className="hook-form" onSubmit={async e=>{e.preventDefault();await mutate(async()=>{if(draft.id){await update(draft)}else{const saved=await api<Hook>(path,'POST',draft);setHooks(v=>ordered([...v,saved]))}setDraft(null)})}}>
   {formError&&<div role="alert"><p className="backup-error">{formError}</p>{draft.id&&<button type="button" className="text-button" disabled={busy} onClick={()=>mutate(async()=>{const items=await api<Hook[]>(path);setHooks(items);const latest=items.find(h=>h.id===draft.id);if(!latest)throw Error('Hook removido. Feche esta edição.');setDraft(latest)})}>Carregar versão atual e descartar esta edição</button>}</div>}
   <label>Nome<input required maxLength={120} value={draft.name} onChange={e=>setDraft({...draft,name:e.target.value})}/></label>
   <label>Momento<select value={draft.event} onChange={e=>{const next=events.find(v=>v.id===e.target.value);setDraft({...draft,event:e.target.value,tools:next?.tool_filter?(draft.tools.length?draft.tools:['fs.apply_patch']):[]})}}>{events.map(e=><option key={e.id} value={e.id}>{e.label}</option>)}</select></label>{draft.event==='before_final'?<p className="field-hint">A resposta pode já estar visível por streaming. Este hook controla a conclusão da execução.</p>:!selectedEvent?.tool_filter&&<p className="field-hint">Este hook é acionado a cada consulta ao modelo, incluindo as rodadas após ferramentas.</p>}
   {selectedEvent?.tool_filter&&<fieldset><legend>Ferramentas que acionam este hook</legend><div className="hook-tools">{tools.map(t=><label key={t.name}><input type="checkbox" checked={draft.tools.includes(t.name)} onChange={e=>setDraft({...draft,tools:e.target.checked?[...draft.tools,t.name]:draft.tools.filter(n=>n!==t.name)})}/>{t.name}</label>)}</div></fieldset>}
   <label>Comando<textarea required maxLength={16000} className="code-input" spellCheck={false} placeholder="pnpm test" value={draft.command} onChange={e=>setDraft({...draft,command:e.target.value})}/></label>
   <label>Diretório relativo ao projeto<input required value={draft.cwd} onChange={e=>setDraft({...draft,cwd:e.target.value})}/></label>
   <label>Tempo limite em segundos<input required type="number" min={1} max={300} value={draft.timeout_seconds} onChange={e=>setDraft({...draft,timeout_seconds:Number(e.target.value)})}/></label>
   <p className="field-hint">O comando será apresentado para aprovação quando o hook for acionado. Salvar esta configuração não executa o comando. Não inclua segredos.</p>
   <footer><button className="primary-button" disabled={busy||!selectedEvent||(selectedEvent.tool_filter&&draft.tools.length===0)}>{busy?'Salvando…':draft.id?'Salvar alterações':'Salvar hook'}</button></footer>
  </form></Modal>}
 </div>;
}
