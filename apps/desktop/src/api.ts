import { invoke, isTauri } from '@tauri-apps/api/core';
export class ApiRequestError extends Error{code:string;status:number;retryable:boolean;correlationId?:string;constructor(message:string,code='request_failed',status=0,retryable=false,correlationId?:string){super(message);this.name='ApiRequestError';this.code=code;this.status=status;this.retryable=retryable;this.correlationId=correlationId}}
export async function api<T=any>(path:string,method='GET',body?:unknown):Promise<T>{
 if(isTauri())try{return await invoke<T>('request',{path,method,body:body??null})}catch(error){if(typeof error==='string'&&error.trim().startsWith('{'))try{const value=JSON.parse(error);throw new ApiRequestError(value.message??'Não foi possível concluir a operação',value.code,0,!!value.retryable,value.correlation_id)}catch(parsed){if(parsed instanceof ApiRequestError)throw parsed}throw error}
 const r=await fetch('/api'+path,{method,headers:{'content-type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});
 const v=await r.json();if(!r.ok)throw new ApiRequestError(v.message??'Não foi possível concluir a operação',v.code,r.status,!!v.retryable,v.correlation_id);return v;
}
export type FolderChoice={status:'selected';path:string}|{status:'cancelled'}|{status:'unsupported'}|{status:'failed';message:string};
export async function chooseFolder():Promise<FolderChoice>{
 if(!isTauri())return {status:'unsupported'};
 try{const {open}=await import('@tauri-apps/plugin-dialog');const path=await open({directory:true,multiple:false}) as string|null;return path?{status:'selected',path}:{status:'cancelled'};}catch(error){return {status:'failed',message:error instanceof Error?error.message:String(error)};}
}
export async function windowAction(action:'minimize'|'toggleMaximize'|'close'){
 if(isTauri()){const {getCurrentWindow}=await import('@tauri-apps/api/window');await getCurrentWindow()[action]();}
}
