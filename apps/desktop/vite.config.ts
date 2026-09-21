import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
export default defineConfig({
 plugins:[react(),{name:'forja-local-bridge',configureServer(server){
  server.middlewares.use('/api',async(req,res,next)=>{
   if(!req.url?.startsWith('/v1/'))return next();
   const origin=req.headers.origin;
   if((origin&&origin!=='http://127.0.0.1:1420'&&origin!=='http://localhost:1420')||!['127.0.0.1:1420','localhost:1420'].includes(req.headers.host??'')){res.statusCode=403;res.end('Origin denied');return;}
   try{
    const base=process.env.FORJA_DATA_DIR??path.join(process.env.LOCALAPPDATA??path.join(os.homedir(),'.local/share'),'Forja');
    const endpoint=JSON.parse(fs.readFileSync(path.join(base,'daemon.json'),'utf8'));
    let body='';for await(const chunk of req){body+=chunk;if(body.length>3_000_000)throw Error('Limite de entrada');}
    const response=await fetch(endpoint.url+req.url,{method:req.method,headers:{authorization:'Bearer '+endpoint.token,'content-type':'application/json'},body:['GET','HEAD'].includes(req.method??'GET')?undefined:body,redirect:'error'});
    res.statusCode=response.status;res.setHeader('content-type','application/json');res.end(await response.text());
   }catch(e){res.statusCode=503;res.setHeader('content-type','application/json');res.end(JSON.stringify({message:'Daemon indisponível. Inicie cargo run -p forja-daemon.',detail:String(e)}));}
  });
 }}],server:{host:'127.0.0.1',port:1420,strictPort:true},clearScreen:false,build:{target:'es2022',chunkSizeWarningLimit:1800}
});
