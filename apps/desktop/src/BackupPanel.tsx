import {useState} from 'react';
import {Download,ShieldCheck,FolderOpen} from 'lucide-react';
import {api,chooseFolder} from './api';

export function BackupPanel(){
 const [source,setSource]=useState(''),[destination,setDestination]=useState(''),[busy,setBusy]=useState(false),[message,setMessage]=useState(''),[error,setError]=useState('');
 async function run(action:()=>Promise<void>){setBusy(true);setMessage('');setError('');try{await action()}catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 return <section className="backup-panel" aria-label="Backup e recuperação">
  <div className="setting-row"><div><h3>Backup e recuperação</h3><p>Cópia consistente de sessões, configurações, checkpoints e artefatos. Credenciais e arquivos de trabalho do projeto ficam fora da cópia.</p></div><button className="subtle-button" disabled={busy} onClick={()=>void run(async()=>{const result=await api<{path:string}>('/v1/backup','POST',{});setSource(result.path);setMessage('Backup salvo em '+result.path)})}><Download size={16}/>{busy?'Aguarde…':'Criar backup'}</button></div>
  <details><summary>Verificar ou restaurar uma cópia</summary>
   <label>Pasta do backup<div className="input-with-button"><input value={source} onChange={e=>{setSource(e.target.value);setMessage('')}} placeholder="C:/Backups/backup-…"/><button aria-label="Selecionar pasta do backup" disabled={busy} onClick={()=>void run(async()=>{const result=await chooseFolder();if(result.status==='selected')setSource(result.path);else if(result.status==='unsupported')setMessage('Informe o caminho da pasta do backup no campo acima. O seletor nativo está disponível no aplicativo desktop.');else if(result.status==='failed')throw Error(result.message)})}><FolderOpen size={16}/></button></div></label>
   <button className="subtle-button" disabled={busy||!source} onClick={()=>void run(async()=>{const result=await api<{files:number;bytes:number}>('/v1/backup/verify','POST',{source});setMessage(`Integridade verificada: ${result.files} arquivo${result.files===1?'':'s'} · ${(result.bytes/1024/1024).toLocaleString('pt-BR',{maximumFractionDigits:1})} MB`)})}><ShieldCheck size={16}/>Verificar integridade</button>
   <label>Nova pasta de dados<input value={destination} onChange={e=>setDestination(e.target.value)} placeholder="D:/Forja-restaurado"/></label>
   <p className="field-hint">Escolha uma pasta que ainda não exista. A restauração cria uma cópia separada e preserva os dados em uso. Para ativá-la, encerre o FORJA, defina FORJA_DATA_DIR com o caminho restaurado e reinicie.</p>
   <button className="primary-button" disabled={busy||!source||!destination} onClick={()=>void run(async()=>{const result=await api<{path:string}>('/v1/backup/restore','POST',{source,destination,confirmed:true});setMessage('Cópia restaurada e validada em '+result.path+'. Os dados em uso continuam na pasta atual.')})}>Restaurar na nova pasta</button>
  </details>
  {message&&<p className="backup-result" role="status">{message}</p>}{error&&<p className="backup-error" role="alert">{error}</p>}
 </section>
}
