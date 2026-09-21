import {attachLsp} from './lsp';
import {useRef,useEffect,useState} from 'react';
import Editor,{DiffEditor,loader} from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import CssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker';
import HtmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker';
import TsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker';
(self as any).MonacoEnvironment={getWorker(_:string,label:string){if(label==='json')return new JsonWorker();if(['css','scss','less'].includes(label))return new CssWorker();if(['html','handlebars'].includes(label))return new HtmlWorker();if(['typescript','javascript'].includes(label))return new TsWorker();return new EditorWorker();}};
loader.config({monaco});
// Project semantics come from the authorized language server, which can resolve dependencies.
for(const defaults of [monaco.languages.typescript.typescriptDefaults,monaco.languages.typescript.javascriptDefaults]){
 defaults.setDiagnosticsOptions({noSemanticValidation:true,noSyntaxValidation:false,noSuggestionDiagnostics:true});
}
monaco.editor.defineTheme('forja',{base:'vs-dark',inherit:true,rules:[],colors:{'editor.background':'#0c131b','editor.lineHighlightBackground':'#121e2b','editorLineNumber.foreground':'#526275','editor.selectionBackground':'#193f62','editorCursor.foreground':'#ff6747','editorIndentGuide.background1':'#1b2938'}});
export function CodePane({path,value,onChange,onSave,location,workspace,onOpen}:{workspace:string;onOpen:(path:string,line:number)=>void;path:string;value:string;location?:{line:number;nonce:number};onChange:(s:string)=>void;onSave:()=>void}){
 const [mountedEditor,setMountedEditor]=useState<monaco.editor.IStandaloneCodeEditor|null>(null);
 const openRef=useRef(onOpen);openRef.current=onOpen;
 useEffect(()=>{if(mountedEditor)return attachLsp(monaco,mountedEditor,workspace,path,(p,l)=>openRef.current(p,l));},[mountedEditor,workspace,path]);
 const save=useRef(onSave);save.current=onSave;
 const editorRef=useRef<monaco.editor.IStandaloneCodeEditor|null>(null);
 const navigate=()=>{if(location&&editorRef.current){const line=Math.min(location.line,editorRef.current.getModel()?.getLineCount()??1);editorRef.current.setPosition({lineNumber:Math.max(1,line),column:1});editorRef.current.revealLineInCenter(line);editorRef.current.focus();}};
 useEffect(navigate,[path,location?.nonce]);
 return <Editor path={'forja:///'+encodeURIComponent(workspace)+'/'+path} value={value} theme="forja" onChange={v=>onChange(v??'')} onMount={editor=>{editorRef.current=editor;setMountedEditor(editor);editor.addCommand(monaco.KeyMod.CtrlCmd|monaco.KeyCode.KeyS,()=>save.current());navigate();}} options={{fontSize:13,fontFamily:"'Cascadia Code', Consolas, monospace",minimap:{enabled:false},padding:{top:16},scrollBeyondLastLine:false,automaticLayout:true}}/>;
}
export function DiffPane({before,after,path}:{before:string;after:string;path:string}){return <DiffEditor original={before} modified={after} originalModelPath={'before/'+path} modifiedModelPath={'after/'+path} theme="forja" options={{readOnly:true,renderSideBySide:true,minimap:{enabled:false},automaticLayout:true}}/>}
