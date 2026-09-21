import {useEffect,useRef,useState} from 'react';
import {FileText,GraduationCap,RefreshCw,X} from 'lucide-react';
import {api} from './api';
import {Modal} from './ui';
import type {Workspace} from './types';

export type SkillChoice={path:string;hash:string;name:string};
export type SkillEntry={name:string;description:string;content:string;path:string;valid:boolean;error?:string;hash?:string};
type Resource={path:string;size:number;within_size_limit:boolean};

export function SkillResources({workspace,skill}:{workspace:string;skill:SkillEntry}){
 const [resources,setResources]=useState<Resource[]|null>(null),[truncated,setTruncated]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[preview,setPreview]=useState<{source:string;hash:string;content:string}|null>(null);
 const request=useRef(0);
 useEffect(()=>()=>{request.current++},[]);
 async function load(resource?:string){
  const current=++request.current;setBusy(true);setError('');setPreview(null);
  try{
   const value=await api('/v1/workspaces/'+workspace+'/skills/'+(resource?'read':'resources'),'POST',{path:skill.path,hash:skill.hash,...(resource?{resource}:{})});
   if(current!==request.current)return;
   if(resource)setPreview(value);else{setResources(value.resources);setTruncated(value.truncated)}
  }catch(e){if(current===request.current)setError(e instanceof Error?e.message:String(e))}
  finally{if(current===request.current)setBusy(false)}
 }
 return <div className="skill-resources"><button className="subtle-button" disabled={busy} onClick={()=>load()}><FileText size={14}/>{resources?'Atualizar recursos':'Explorar recursos'}</button><p className="field-hint">Prévia de texto UTF-8, até 64 KB. Abrir um script apenas mostra seu conteúdo.</p>
  {busy&&<p role="status">Carregando…</p>}{error&&<p className="backup-error" role="alert">{error}</p>}
  {resources&&<div className="skill-resource-list">{resources.map(r=><button key={r.path} disabled={busy||!r.within_size_limit} onClick={()=>load(r.path)}><FileText size={14}/><span>{r.path}</span><small>{r.within_size_limit?r.size.toLocaleString('pt-BR')+' bytes':'Excede 64 KB'}</small></button>)}{!resources.length&&<p>Nenhum recurso disponível.</p>}{truncated&&<p role="status">Lista parcial: limite de arquivos ou profundidade atingido.</p>}</div>}
  {preview&&<section className="skill-preview" aria-label="Prévia do recurso"><strong>{preview.source}</strong><small>SHA-256: {preview.hash}</small><pre>{preview.content}</pre></section>}
 </div>
}

export function SkillPicker({workspace,selected,onApply,onClose}:{workspace:Workspace;selected:SkillChoice[];onApply:(value:SkillChoice[])=>void;onClose:()=>void}){
 const [skills,setSkills]=useState<SkillEntry[]>([]),[chosen,setChosen]=useState(selected),[query,setQuery]=useState(''),[loading,setLoading]=useState(true),[error,setError]=useState('');
 const request=useRef(0);
 async function refresh(){const current=++request.current;setLoading(true);setError('');try{const result=await api<SkillEntry[]>('/v1/workspaces/'+workspace.id+'/skills');if(current===request.current)setSkills(result)}catch(e){if(current===request.current)setError(e instanceof Error?e.message:String(e))}finally{if(current===request.current)setLoading(false)}}
 useEffect(()=>{refresh();return()=>{request.current++}},[workspace.id]);
 const stale=chosen.filter(c=>!skills.some(s=>s.valid&&s.path===c.path&&s.hash===c.hash));
 function toggle(s:SkillEntry){setChosen(old=>old.some(c=>c.path===s.path)?old.filter(c=>c.path!==s.path):[...old,{path:s.path,hash:s.hash!,name:s.name}])}
 return <Modal title="Skills para este envio" className="skill-picker-modal" onClose={onClose}><p className="modal-description">Escolha até oito skills de {workspace.name}. As instruções escolhidas entram no contexto deste envio; recursos continuam sendo carregados sob demanda. A seleção não concede permissões.</p>
  <div className="skill-picker-toolbar"><input aria-label="Buscar skills" placeholder="Buscar por nome ou descrição…" value={query} onChange={e=>setQuery(e.target.value)}/><button className="icon-button" aria-label="Atualizar catálogo de skills" disabled={loading} onClick={refresh}><RefreshCw size={17}/></button></div>
  <p className="field-hint" role="status">{loading?'Carregando catálogo…':`${chosen.length} de 8 selecionadas`}</p>{error&&<p className="backup-error" role="alert">{error}</p>}
  {!loading&&stale.length>0&&<div className="skill-stale" role="alert"><p>Uma skill selecionada mudou ou não está disponível. Atualize a seleção para usar o conteúdo atual, ou remova-a.</p>{stale.map(c=><div key={c.path}><span>{c.name}</span><button aria-label={'Remover seleção '+c.name} onClick={()=>setChosen(old=>old.filter(s=>s.path!==c.path))}><X size={14}/></button></div>)}</div>}
  <div className="skill-picker-list">{skills.filter(s=>(s.name+' '+s.description+' '+s.path).toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(s=>{
   const choice=chosen.find(c=>c.path===s.path),changed=choice&&choice.hash!==s.hash;
   return <article className="skill-picker-row" key={s.path}><label><input type="checkbox" checked={!!choice} disabled={!s.valid||loading||(!choice&&chosen.length>=8)} onChange={()=>toggle(s)}/><span><strong>{s.name||s.path}</strong><small>{s.description||s.error}</small></span></label><small className="skill-source">{s.path}</small>
    {changed&&s.valid&&<button className="text-button" onClick={()=>setChosen(old=>old.map(c=>c.path===s.path?{path:s.path,hash:s.hash!,name:s.name}:c))}>Atualizar seleção de {s.name}</button>}
    {s.valid&&<details><summary>Inspecionar instruções e recursos</summary><small className="skill-source">SHA-256: {s.hash}</small><pre className="skill-body">{s.content}</pre><SkillResources key={s.path+':'+s.hash} workspace={workspace.id} skill={s}/></details>}
   </article>
  })}{!loading&&!skills.length&&!error&&<div className="small-empty"><GraduationCap size={25}/><p>Nenhuma skill neste projeto. Crie uma no painel Skills.</p></div>}</div>
  <footer><button className="subtle-button" onClick={onClose}>Cancelar</button><button className="primary-button" disabled={loading||!!error||stale.length>0} onClick={()=>onApply(chosen)}>Usar seleção</button></footer>
 </Modal>
}
