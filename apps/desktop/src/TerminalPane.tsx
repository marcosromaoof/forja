import {useEffect,useRef,useState} from 'react';
import {Terminal} from '@xterm/xterm';
import {FitAddon} from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import {api} from './api';
import {TerminalSquare} from 'lucide-react';
export function TerminalPane({workspace,onError}:{workspace:string;onError:(e:unknown)=>void}){
 const host=useRef<HTMLDivElement>(null);const [id,setId]=useState('');const [starting,setStarting]=useState(false);
 useEffect(()=>{if(!id||!host.current)return;let disposed=false;let previous='';let busy=false;const term=new Terminal({theme:{background:'#0c131b',foreground:'#c8d7e8',cursor:'#ff6848'},fontSize:12,fontFamily:"'Cascadia Code',Consolas,monospace",cursorBlink:true,convertEol:true});const fit=new FitAddon();term.loadAddon(fit);term.open(host.current);fit.fit();
 const input=term.onData(input=>{api('/v1/terminals/'+id+'/input','POST',{input}).catch(onError)});
 const observer=new ResizeObserver(()=>{if(disposed)return;fit.fit();api('/v1/terminals/'+id+'/resize','POST',{rows:term.rows,cols:term.cols}).catch(()=>{});});observer.observe(host.current);
 const timer=setInterval(async()=>{if(busy||disposed)return;busy=true;try{const r=await api<{output:string}>('/v1/terminals/'+id);if(!disposed){if(r.output.startsWith(previous))term.write(r.output.slice(previous.length));else{term.clear();term.write(r.output);}previous=r.output;}}catch(e){if(!disposed){clearInterval(timer);onError(e)}}finally{busy=false}},250);
 return()=>{disposed=true;clearInterval(timer);observer.disconnect();input.dispose();term.dispose();api('/v1/terminals/'+id,'DELETE').catch(()=>{});};
 },[id]);
 return <div className="terminal-pane">{!id?<div className="terminal-empty"><TerminalSquare size={24}/><span>Terminal manual do projeto</span><small>PowerShell · isolamento reduzido</small><button className="subtle-button" disabled={starting} onClick={async()=>{setStarting(true);try{const r=await api('/v1/terminals','POST',{workspace_id:workspace});setId(r.id)}catch(e){onError(e)}finally{setStarting(false)}}}>{starting?'Iniciando…':'Abrir terminal'}</button></div>:<div ref={host} className="xterm-host"/>}</div>
}
