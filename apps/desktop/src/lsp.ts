import type * as Monaco from 'monaco-editor';
import {api} from './api';
type Position={line:number;character:number};
type Range={start:Position;end:Position};
type Diagnostic={range:Range;message:string;severity?:number;source?:string};
type Hover={contents?:string|{value:string}|({value:string}|string)[]};
type Completion={label:string|{label:string};detail?:string;insertText?:string;insertTextFormat?:number;textEdit?:{newText:string;range?:Range};documentation?:string|{value:string}};
const convertRange=(r:Range)=>({startLineNumber:r.start.line+1,startColumn:r.start.character+1,endLineNumber:r.end.line+1,endColumn:r.end.character+1});
const plain=(c:Hover['contents']):string=>Array.isArray(c)?c.map(plain).join('\n\n'):typeof c==='string'?c:c?.value??'';

export function attachLsp(monaco:typeof Monaco,editor:Monaco.editor.IStandaloneCodeEditor,workspace:string,path:string,onOpen:(path:string,line:number)=>void){
 const extension=path.split('.').pop()?.toLowerCase();
 const language=extension==='py'?'python':['ts','tsx','js','jsx','mjs','cjs'].includes(extension??'')?'typescript':null;
 if(!language)return ()=>{};
 const model=editor.getModel();if(!model)return ()=>{};
 const endpoint='/v1/workspaces/'+workspace+'/lsp/'+language;
 const owner='forja-lsp-'+workspace;
 let disposed=false,active=false,polling=false,epoch=0,debounce:ReturnType<typeof setTimeout>|undefined;
 const registrations:Monaco.IDisposable[]=[];
 async function sync(){if(!active||disposed||model!.isDisposed())return;await api(endpoint+'/sync','POST',{path,text:model!.getValue()});}
 async function query(method:string,position:Monaco.Position){
  if(!active||disposed||model!.isDisposed())return null;
  const version=model!.getVersionId();
  const result=await api(endpoint+'/query','POST',{method,path,text:model!.getValue(),line:position.lineNumber-1,character:position.column-1});
  return disposed||model!.isDisposed()||version!==model!.getVersionId()?null:result;
 }
 function clear(){while(registrations.length)registrations.pop()?.dispose();if(!model!.isDisposed())monaco.editor.setModelMarkers(model!,owner,[]);}
 function install(){
  registrations.push(monaco.languages.registerHoverProvider(model!.getLanguageId(),{async provideHover(m,p,token){
   if(m!==model||token.isCancellationRequested)return null;
   try{const result=await query('textDocument/hover',p) as Hover|null;if(!result||token.isCancellationRequested)return null;return {contents:[{value:plain(result.contents),isTrusted:false,supportHtml:false}]};}catch{return null}
  }}));
  registrations.push(monaco.languages.registerCompletionItemProvider(model!.getLanguageId(),{triggerCharacters:['.'],async provideCompletionItems(m,p,_context,token){
   if(m!==model||token.isCancellationRequested)return {suggestions:[]};
   try{
    const result=await query('textDocument/completion',p);if(token.isCancellationRequested||!result)return {suggestions:[]};
    const entries:Completion[]=Array.isArray(result)?result:result.items??[];
    const word=m.getWordUntilPosition(p);const fallback={startLineNumber:p.lineNumber,endLineNumber:p.lineNumber,startColumn:word.startColumn,endColumn:word.endColumn};
    return {suggestions:entries.slice(0,300).filter(item=>item.insertTextFormat!==2).map(item=>({label:typeof item.label==='string'?item.label:item.label.label,kind:monaco.languages.CompletionItemKind.Text,detail:item.detail,insertText:item.textEdit?.newText??item.insertText??(typeof item.label==='string'?item.label:item.label.label),range:item.textEdit?.range?convertRange(item.textEdit.range):fallback,documentation:{value:plain(item.documentation),isTrusted:false,supportHtml:false}}))};
   }catch{return {suggestions:[]}}
  }}));
  registrations.push(editor.addAction({id:'forja.goToDefinition',label:'Ir para definição (LSP)',keybindings:[monaco.KeyCode.F12],contextMenuGroupId:'navigation',run:async()=>{
   const position=editor.getPosition();if(!position)return;
   try{const result=await query('textDocument/definition',position) as {path:string;range:Range}[]|null;if(result?.[0]?.range)onOpen(result[0].path,result[0].range.start.line+1);}catch{/* The connection state is reflected by the next status poll. */}
  }}));
 }
 async function poll(){
  if(polling||disposed)return;polling=true;const ticket=epoch;
  try{
   const statuses=await api<{language:string;active:boolean}[]>('/v1/workspaces/'+workspace+'/lsp');
   if(disposed||ticket!==epoch)return;
   const enabled=statuses.some(s=>s.language===language&&s.active);
   if(enabled!==active){active=enabled;clear();if(active){install();await sync();}}
   if(active){const version=model!.getVersionId();const reports=await api<{path:string;diagnostics:Diagnostic[]}[]>(endpoint+'/diagnostics');if(disposed||model!.isDisposed()||version!==model!.getVersionId())return;
    const report=reports.find(r=>r.path===path);
    monaco.editor.setModelMarkers(model!,owner,(report?.diagnostics??[]).map(d=>({...convertRange(d.range),message:d.message,source:d.source??'LSP',severity:d.severity===1?monaco.MarkerSeverity.Error:d.severity===2?monaco.MarkerSeverity.Warning:monaco.MarkerSeverity.Info})));
   }
  }catch{active=false;clear();}finally{polling=false}
 }
 const changed=model.onDidChangeContent(()=>{if(debounce)clearTimeout(debounce);debounce=setTimeout(()=>{sync().catch(()=>{});},300)});
 void poll();const interval=setInterval(()=>{void poll()},1500);
 return ()=>{disposed=true;epoch++;clearInterval(interval);if(debounce)clearTimeout(debounce);changed.dispose();clear();};
}
