import {useEffect,useState} from 'react';
import {Globe2,KeyRound,Plus,Save,Server,Trash2} from 'lucide-react';
import {api} from './api';
import type {SearchProvider} from './types';
import './SearchProviderPanel.css';

type Draft={id:string;kind:SearchProvider['kind'];name:string;baseUrl:string;apiKey:string;enabled:boolean;allowLocal:boolean;revision:number;hasSecret:boolean};
const presets:Record<SearchProvider['kind'],Pick<Draft,'name'|'baseUrl'|'allowLocal'>>={
 brave:{name:'Brave Search',baseUrl:'https://api.search.brave.com/res/v1/web/search',allowLocal:false},
 searxng:{name:'SearXNG',baseUrl:'http://127.0.0.1:8080/',allowLocal:true},
 declarative:{name:'Busca REST',baseUrl:'https://',allowLocal:false}
};
const fresh=(kind:SearchProvider['kind']='brave'):Draft=>({id:'',kind,...presets[kind],apiKey:'',enabled:true,revision:0,hasSecret:false});

export function SearchProviderPanel({onError}:{onError:(error:unknown)=>void}){
 const [providers,setProviders]=useState<SearchProvider[]>([]),[selected,setSelected]=useState(''),[draft,setDraft]=useState<Draft>(()=>fresh()),[busy,setBusy]=useState(false),[loaded,setLoaded]=useState(false);
 async function refresh(){try{const value=await api<SearchProvider[]>('/v1/search-providers');setProviders(value);setLoaded(true)}catch(error){onError(error)}}
 useEffect(()=>{void refresh()},[]);
 function edit(provider:SearchProvider){setSelected(provider.id);setDraft({id:provider.id,kind:provider.kind,name:provider.name,baseUrl:provider.base_url,apiKey:'',enabled:provider.enabled,allowLocal:provider.allow_local,revision:provider.revision,hasSecret:!!provider.secret_ref})}
 function changeKind(kind:SearchProvider['kind']){const preset=presets[kind];setDraft(value=>({...value,kind,name:value.id?value.name:preset.name,baseUrl:value.id?value.baseUrl:preset.baseUrl,allowLocal:kind==='searxng'?true:value.id?value.allowLocal:preset.allowLocal}))}
 async function save(){if(!draft.name.trim()||!draft.baseUrl.trim())return;setBusy(true);try{const value=await api<SearchProvider>('/v1/search-providers','POST',{id:draft.id,kind:draft.kind,name:draft.name.trim(),base_url:draft.baseUrl.trim(),enabled:draft.enabled,allow_local:draft.allowLocal,revision:draft.revision,api_key:draft.apiKey});setProviders(items=>[value,...items.filter(item=>item.id!==value.id)]);edit(value)}catch(error){onError(error)}finally{setBusy(false)}}
 async function remove(){if(!selected||!window.confirm('Excluir este provedor de pesquisa e sua credencial local?'))return;setBusy(true);try{await api('/v1/search-providers/'+selected,'DELETE');setProviders(items=>items.filter(item=>item.id!==selected));setSelected('');setDraft(fresh())}catch(error){onError(error)}finally{setBusy(false)}}
 return <section className="search-provider-panel" aria-labelledby="search-provider-title">
  <header><div><Globe2 size={21}/><span><h3 id="search-provider-title">Pesquisa na web</h3><p>Configure o mecanismo usado por agentes autorizados. Resultados são tratados como conteúdo não confiável.</p></span></div><button className="subtle-button" onClick={()=>{setSelected('');setDraft(fresh())}}><Plus size={15}/>Adicionar</button></header>
  <div className="search-provider-layout">
   <aside aria-label="Provedores de pesquisa">{providers.map(provider=><button key={provider.id} className={selected===provider.id?'active':''} onClick={()=>edit(provider)}><Server size={16}/><span><strong>{provider.name}</strong><small>{provider.kind==='brave'?'Brave Search':provider.kind==='searxng'?'SearXNG':'REST declarativo'}</small></span><i className={provider.enabled?'green-dot':'idle-dot'}/></button>)}{loaded&&!providers.length&&<p>Nenhum buscador configurado. Adicione Brave, SearXNG ou um endpoint REST compatível.</p>}</aside>
   <div className="search-provider-form">
    <div className="search-provider-grid"><label>Adaptador<select value={draft.kind} onChange={event=>changeKind(event.target.value as SearchProvider['kind'])}><option value="brave">Brave Search</option><option value="searxng">SearXNG</option><option value="declarative">REST declarativo</option></select></label><label>Nome<input value={draft.name} onChange={event=>setDraft(value=>({...value,name:event.target.value}))} placeholder="Pesquisa principal"/></label></div>
    <label>Endpoint<input value={draft.baseUrl} onChange={event=>setDraft(value=>({...value,baseUrl:event.target.value}))} placeholder="https://servidor/search"/></label>
    {draft.kind==='brave'&&<label>Chave da API<span className="secret-input"><KeyRound size={15}/><input type="password" autoComplete="new-password" value={draft.apiKey} onChange={event=>setDraft(value=>({...value,apiKey:event.target.value}))} placeholder={draft.hasSecret?'Deixe vazio para manter a credencial atual':'Obrigatória para o Brave Search'}/></span></label>}
    <div className="search-provider-options"><label><input type="checkbox" checked={draft.enabled} onChange={event=>setDraft(value=>({...value,enabled:event.target.checked}))}/>Disponível para novos agentes</label><label><input type="checkbox" checked={draft.allowLocal} onChange={event=>setDraft(value=>({...value,allowLocal:event.target.checked}))}/>Permitir endpoint na rede local</label></div>
    {draft.allowLocal&&<p className="field-hint">O endpoint poderá receber consultas dentro da sua rede. Use esta opção para uma instância SearXNG administrada por você.</p>}
    <footer>{selected?<button className="danger-text" disabled={busy} onClick={remove}><Trash2 size={15}/>Excluir</button>:<span/>}<button className="primary-button" disabled={busy||!draft.name.trim()||!draft.baseUrl.trim()||(draft.kind==='brave'&&!draft.hasSecret&&!draft.apiKey)} onClick={save}><Save size={15}/>{busy?'Salvando…':'Salvar buscador'}</button></footer>
   </div>
  </div>
 </section>
}
