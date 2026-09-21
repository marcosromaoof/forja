import {useEffect,useState} from 'react';
import {AlertCircle,AlertTriangle,Info} from 'lucide-react';
import {api} from './api';

type Diagnostic={message:string;severity?:number;source?:string;range:{start:{line:number;character:number}}};
type Report={path:string;diagnostics:Diagnostic[]};

export function ProblemsPanel({workspace,onOpen}:{workspace:string;onOpen:(path:string,line:number)=>void}){
 const [reports,setReports]=useState<Report[]>([]),[active,setActive]=useState(0),[error,setError]=useState(''),[loading,setLoading]=useState(true);
 useEffect(()=>{
  let disposed=false,pending=false;
  async function refresh(){
   if(pending)return;pending=true;
   try{
    const status=await api<{language:string;active:boolean}[]>('/v1/workspaces/'+workspace+'/lsp');
    const running=status.filter(s=>s.active);
    const values=await Promise.all(running.map(s=>api<Report[]>('/v1/workspaces/'+workspace+'/lsp/'+s.language+'/diagnostics')));
    if(!disposed){setActive(running.length);setReports(values.flat());setError('')}
   }catch(e){if(!disposed){setReports([]);setError(e instanceof Error?e.message:String(e))}}
   finally{pending=false;if(!disposed)setLoading(false)}
  }
  void refresh();const timer=setInterval(()=>{void refresh()},1500);
  return()=>{disposed=true;clearInterval(timer)};
 },[workspace]);
 const count=reports.reduce((n,r)=>n+r.diagnostics.length,0);
 return <section className="problems-panel" aria-label="Diagnósticos de código">
  <p className="problems-summary" role="status">{loading?'Consultando servidores…':error?'Não foi possível consultar diagnósticos: '+error:active===0?'Ative um servidor em Contexto → Inteligência de código.':count===0?'Nenhum diagnóstico recebido para os documentos abertos.':`${count} diagnóstico${count===1?'':'s'} nos documentos abertos`}</p>
  {reports.filter(r=>r.diagnostics.length).map(report=><div key={report.path} className="problem-file"><strong>{report.path}</strong>{report.diagnostics.map((d,i)=><button key={i} onClick={()=>onOpen(report.path,d.range.start.line+1)} className={'problem-row severity-'+(d.severity??3)}>
   {d.severity===1?<AlertCircle size={14}/>:d.severity===2?<AlertTriangle size={14}/>:<Info size={14}/>}
   <span>{d.message}<small>{d.source??'LSP'} · Linha {d.range.start.line+1}, coluna {d.range.start.character+1}</small></span>
  </button>)}</div>)}
 </section>
}
