import {LspControls} from './LspControls';
import {useEffect,useState} from 'react';
import {FileCode,RefreshCw,Search} from 'lucide-react';
import {api} from './api';
import type {Workspace} from './types';
type Symbol={name:string;kind:string;line:number;end_line:number};
type RepoMap={indexed_files:number;indexed_bytes?:number;truncated?:boolean;created_at?:string;files:{path:string;symbols:Symbol[]}[]};
export function ContextPanel({workspace,onOpen,onError}:{workspace:Workspace|null;onOpen:(path:string,line?:number)=>void;onError:(e:unknown)=>void}){
 const [map,setMap]=useState<RepoMap>({indexed_files:0,files:[]}),[query,setQuery]=useState(''),[busy,setBusy]=useState(false);
 useEffect(()=>{setMap({indexed_files:0,files:[]});if(workspace)api<RepoMap>('/v1/workspaces/'+workspace.id+'/map').then(setMap).catch(onError)},[workspace?.id]);
 if(!workspace)return <div className="empty-state"><FileCode size={36}/><h2>Abra um projeto</h2><p>O mapa reúne arquivos e símbolos do workspace autorizado.</p></div>;
 const matches=map.files.filter(f=>f.path.toLowerCase().includes(query.toLowerCase())||f.symbols.some(s=>s.name.toLowerCase().includes(query.toLowerCase())));
 return <div className="integration-panel"><LspControls workspace={workspace.id} onError={onError}/><div className="integration-toolbar"><div><strong>{workspace.name}</strong><p>{map.indexed_files} arquivos indexados{map.created_at?' · '+new Date(map.created_at).toLocaleString('pt-BR'):''}</p></div><button className="primary-button" disabled={busy} onClick={async()=>{setBusy(true);try{setMap(await api('/v1/workspaces/'+workspace.id+'/index','POST',{}))}catch(e){onError(e)}finally{setBusy(false)}}}><RefreshCw size={16}/>{busy?'Indexando…':'Atualizar índice'}</button></div><p className="field-hint">Símbolos de TypeScript, JavaScript e Python. Arquivos ignorados e caminhos protegidos são excluídos. Use @caminho/do/arquivo no objetivo para incluir uma referência com origem e hash.</p><label className="context-search"><Search size={18}/><input aria-label="Filtrar arquivos e símbolos" value={query} onChange={e=>setQuery(e.target.value)} placeholder="Buscar arquivo, função ou classe…"/></label>
 {matches.slice(0,200).map(file=><details className="symbol-file" key={file.path}><summary><FileCode size={17}/><span>{file.path}</span><small>{file.symbols.length} símbolos</small></summary><button className="text-button" onClick={()=>onOpen(file.path)}>Abrir arquivo no editor</button>{file.symbols.map((symbol,i)=><button className="symbol-row" key={i} onClick={()=>onOpen(file.path,symbol.line)}><code>{symbol.name}</code><small>Linha {symbol.line} · {symbol.kind.replaceAll('_',' ')}</small></button>)}</details>)}
 {map.truncated&&<p className="cloud-notice">Índice parcial: o projeto excedeu o limite de 5.000 arquivos ou 30 MB de texto.</p>}{matches.length>200&&<p className="field-hint">Mostrando 200 de {matches.length} arquivos. Refine a busca.</p>}{!map.indexed_files&&<div className="empty-state"><FileCode size={34}/><h2>Conheça a estrutura do projeto</h2><p>Atualize o índice para navegar pelas funções, classes e tipos.</p></div>}
 </div>
}
