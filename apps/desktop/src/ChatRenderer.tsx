import {useEffect,useMemo,useState,type CSSProperties} from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {Highlight,themes,type Language} from 'prism-react-renderer';
import {AlertCircle,Check,ChevronDown,Clipboard,FileDiff,Search,SquareTerminal,WrapText} from 'lucide-react';
import {api} from './api';
import {Logo} from './Logo';
import type {ChatItem,ContextState,Interaction} from './types';

function CodeBlock({className,children}:{className?:string;children:unknown}){
 const code=String(children??'').replace(/\n$/,'');const language=(className?.replace('language-','')||'text') as Language;
 const [wrap,setWrap]=useState(false),[copied,setCopied]=useState(false);
 async function copy(){await navigator.clipboard.writeText(code);setCopied(true);setTimeout(()=>setCopied(false),1200)}
 return <div className={'ide-code '+(wrap?'wrap':'')}><header><span>{language}</span><button onClick={()=>setWrap(!wrap)} aria-label="Alternar quebra de linha"><WrapText size={14}/></button><button onClick={copy} aria-label="Copiar código"><Clipboard size={14}/>{copied?'Copiado':'Copiar'}</button></header><Highlight theme={themes.nightOwl} code={code} language={language}>{({tokens,getLineProps,getTokenProps})=><pre>{tokens.map((line,index)=><div key={index} {...getLineProps({line})}><span className="code-line-number">{index+1}</span>{line.map((token,key)=><span key={key} {...getTokenProps({token})}/>)}</div>)}</pre>}</Highlight></div>
}

function Markdown({text}:{text:string}){
 return <div className="message-markdown"><ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={{
  a:({href,children})=><a href={href} target="_blank" rel="noreferrer">{children}</a>,
  code:({className,children})=>className?.startsWith('language-')?<CodeBlock className={className}>{children}</CodeBlock>:<code className="inline-code">{children}</code>
 }}>{text}</ReactMarkdown></div>
}

function JsonTree({value}:{value:unknown}){const [all,setAll]=useState(false);const raw=JSON.stringify(value,null,2)??String(value);const shown=all?raw:raw.slice(0,8000);return <><pre className="tool-json">{shown}{!all&&raw.length>8000?'\n… saída recolhida':''}</pre>{raw.length>8000&&<button className="tool-more" onClick={()=>setAll(!all)}>{all?'Recolher':'Mostrar mais'}</button>}</>}
const cleanAnsi=(value:unknown)=>String(value??'').replace(/[\u001b\u009b][[\]()#;?]*(?:(?:(?:[a-zA-Z\d]*(?:;[-a-zA-Z\d\/#&.:=?%@~_]+)*)?\u0007)|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))/g,'');
function DiffOutput({value}:{value:unknown}){return <pre className="diff-output">{String(value??'').split('\n').map((line,index)=><span key={index} className={line.startsWith('+')&&!line.startsWith('+++')?'diff-add':line.startsWith('-')&&!line.startsWith('---')?'diff-remove':line.startsWith('@@')?'diff-hunk':''}>{line+'\n'}</span>)}</pre>}

function ToolCard({item}:{item:ChatItem}){
 const payload=item.event?.payload??{};const name=String(payload.name??item.text);const output=payload.output??{};const result=output.result??output;const success=output.success!==false;
 const icon=name==='terminal.exec'?<SquareTerminal size={15}/>:name==='fs.apply_patch'?<FileDiff size={15}/>:name.includes('search')?<Search size={15}/>:success?<Check size={15}/>:<AlertCircle size={15}/>;
 const terminal=name==='terminal.exec';const patch=name==='fs.apply_patch';
 return <details className={'tool-card '+(success?'success':'failed')}><summary>{icon}<span><strong>{name}</strong><small>{success?'Concluído':'Falhou'}</small></span><ChevronDown size={14}/></summary><div className="tool-body">{terminal?<><dl className="tool-meta"><dt>Comando</dt><dd>{result.command??payload.arguments?.command??'—'}</dd><dt>Diretório</dt><dd>{result.cwd??payload.arguments?.cwd??'.'}</dd><dt>Saída</dt><dd>{result.exit_code??'—'}</dd></dl>{result.stdout&&<pre className="terminal-output">{cleanAnsi(result.stdout)}</pre>}{result.stderr&&<pre className="terminal-output stderr">{cleanAnsi(result.stderr)}</pre>}{!result.stdout&&!result.stderr&&<JsonTree value={result}/>}</>:patch?<><div className="patch-heading">{payload.arguments?.path??result.path??'Alteração aplicada'}</div><DiffOutput value={result.diff??result.patch??JSON.stringify(result,null,2)}/></>:<JsonTree value={result}/>}</div></details>
}

function ScreenshotCard({item}:{item:ChatItem}){
 const artifact=item.event?.payload?.artifact;const [src,setSrc]=useState(''),[error,setError]=useState('');
 useEffect(()=>{let active=true;if(!artifact?.id)return;api<{media_type:string;base64:string}>('/v1/artifacts/'+artifact.id+'/content').then(value=>{if(active)setSrc(`data:${value.media_type};base64,${value.base64}`)}).catch(value=>{if(active)setError(value instanceof Error?value.message:String(value))});return()=>{active=false}},[artifact?.id]);
 return <figure className="screenshot-card"><figcaption><span>SCREENSHOT DO NAVEGADOR</span><strong>{artifact?.metadata?.title??item.event?.payload?.url??'Interface capturada'}</strong><small>{artifact?.metadata?.url??item.event?.payload?.url}</small></figcaption>{src?<img src={src} alt={'Screenshot de '+(artifact?.metadata?.title??'interface web')}/>:<div className="screenshot-loading">{error||'Carregando artefato visual…'}</div>}</figure>
}

export function MessageRenderer({item}:{item:ChatItem}){
 if(item.event?.type==='browser.screenshot')return <ScreenshotCard item={item}/>;
 if(item.kind==='tool')return <ToolCard item={item}/>;
 if(item.kind==='status')return <div className="chat-status"><AlertCircle size={15}/><span>{item.text}</span></div>;
 return <article className={'ide-message '+item.kind}><header>{item.kind==='user'?<span className="user-mark">V</span>:<Logo/>}<strong>{item.kind==='user'?'Você':'FORJA'}</strong>{item.event?.timestamp&&<time>{new Date(item.event.timestamp).toLocaleTimeString('pt-BR',{hour:'2-digit',minute:'2-digit'})}</time>}</header>{item.text?<Markdown text={item.text}/>:<span className="thinking">Preparando resposta…</span>}</article>
}

export function InteractionCard({interaction,onDone}:{interaction:Interaction;onDone:()=>void}){
 const [value,setValue]=useState<string|string[]>(interaction.kind==='multiple'?[]:''),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const valid=interaction.kind==='multiple'?(value as string[]).length>0||!interaction.required:String(value).trim().length>0||!interaction.required;
 async function submit(){if(!valid)return;setBusy(true);setError('');try{await api('/v1/interactions/'+interaction.id+'/answer','POST',{answer:interaction.kind==='multiple'?value:interaction.kind==='single'?{id:value}:{text:value}});onDone()}catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 return <section className="interaction-card" aria-labelledby={'question-'+interaction.id}><span className="interaction-kicker">DECISÃO NECESSÁRIA</span><h3 id={'question-'+interaction.id}>{interaction.question}</h3>{interaction.detail&&<p>{interaction.detail}</p>}<div className="interaction-options">{interaction.kind==='text'?<textarea value={value as string} onChange={e=>setValue(e.target.value)} placeholder="Digite sua resposta…"/>:interaction.options.map(option=><label key={option.id}><input type={interaction.kind==='single'?'radio':'checkbox'} name={interaction.id} checked={interaction.kind==='single'?value===option.id:(value as string[]).includes(option.id)} onChange={()=>setValue(interaction.kind==='single'?option.id:(value as string[]).includes(option.id)?(value as string[]).filter(v=>v!==option.id):[...(value as string[]),option.id])}/><span><strong>{option.label}</strong>{option.description&&<small>{option.description}</small>}</span></label>)}</div>{error&&<div className="field-error">{error}</div>}<footer><button className="subtle-button" disabled={busy} onClick={async()=>{await api('/v1/interactions/'+interaction.id+'/cancel','POST',{});onDone()}}>Cancelar</button><button className="primary-button" disabled={busy||!valid} onClick={submit}>{busy?'Enviando…':'Continuar planejamento'}</button></footer></section>
}

export function ContextMeter({state,onCompact}:{state:ContextState|null;onCompact:()=>Promise<void>}){
 const [open,setOpen]=useState(false),[busy,setBusy]=useState(false);const percent=Math.round((state?.usage_percent??0)*100);const tone=percent>=85?'danger':percent>=70?'warning':'normal';
 const formatter=useMemo(()=>new Intl.NumberFormat('pt-BR',{notation:'compact',maximumFractionDigits:1}),[]);
 if(!state)return <span className="context-meter unavailable" title="Configure a janela do modelo">Contexto —</span>;
 return <div className="context-wrap"><button className={'context-meter '+tone} onClick={()=>setOpen(!open)} aria-expanded={open}><i style={{'--usage':Math.min(percent,100)+'%'} as CSSProperties}/><span>{formatter.format(state.used_input_tokens)} / {formatter.format(state.usable_input_tokens)}</span><small>{percent}%{state.count_source==='local_estimate'?' · estimado':''}</small></button>{open&&<div className="context-popover"><header><strong>Janela de contexto</strong><span>{percent}%</span></header><div className="context-progress"><i style={{width:Math.min(percent,100)+'%'}}/></div><dl><dt>Sistema</dt><dd>{formatter.format(state.breakdown.system)}</dd><dt>Plano fixado</dt><dd>{formatter.format(state.breakdown.plan??0)}</dd><dt>Checkpoint</dt><dd>{formatter.format(state.breakdown.implementation_checkpoint??0)}</dd><dt>Ferramentas</dt><dd>{formatter.format(state.breakdown.tools)}</dd><dt>Histórico</dt><dd>{formatter.format(state.breakdown.history)}</dd><dt>Saída reservada</dt><dd>{formatter.format(state.reserved_output_tokens)}</dd><dt>Margem</dt><dd>{formatter.format(state.safety_margin_tokens)}</dd><dt>Limite</dt><dd>{state.context_limit_source}</dd></dl><button className="subtle-button" disabled={busy} onClick={async()=>{setBusy(true);try{await onCompact()}finally{setBusy(false)}}}>{busy?'Compactando…':'Compactar agora'}</button></div>}</div>
}
