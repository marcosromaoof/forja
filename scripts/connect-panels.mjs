import fs from 'node:fs';function edit(p,f){fs.writeFileSync(p,f(fs.readFileSync(p,'utf8')))}
edit('apps/desktop/src/App.tsx',s=>{
s="import {McpPanel} from './McpPanel';import {ContextPanel} from './ContextPanel';import {SkillsPanel} from './SkillsPanel';import './integrations.css';\n"+s;
s=s.replace('{Plus,Home,','{Plug,Network,Plus,Home,').replace("'tools'|'skills'","'tools'|'mcp'|'context'|'skills'");
s=s.replace(",[skills,setSkills]=useState<any[]>([])","");
s=s.replace("if(p==='skills'&&workspace)api('/v1/workspaces/'+workspace.id+'/skills').then(setSkills).catch(fail);","");
s=s.replace("function loadSession(s:Session){","function loadSession(s:Session){if(Object.keys(edited).length){fail('Salve os arquivos antes de trocar de tarefa.');return}");
s=s.replace("<button className={mode==='agent'?","<button aria-label=\"Modo Construir\" className={mode==='agent'?").replace("<button className={mode==='plan'?","<button aria-label=\"Modo Planejamento\" className={mode==='plan'?");
s=s.replace('<input type="file" ref={fileInput}','<input aria-label="Anexar arquivos de texto" type="file" ref={fileInput}');
s=s.replace("a.tool.name==='terminal.exec'?'Executar comando no projeto?':'Aplicar alteração no projeto?'","a.tool.name==='mcp.call'?'Autorizar chamada ao servidor MCP?':a.tool.name==='terminal.exec'?'Executar comando no projeto?':'Aplicar alteração no projeto?'");
s=s.replace("Isolamento reduzido · autorização por sessão","{a.tool.name==='mcp.call'?'Acesso externo · somente esta chamada':'Isolamento reduzido · autorização por sessão'}");
s=s.replace('<div className="approval-actions"><button className="primary-button"','<div className="approval-actions">{a.tool.name!==\'mcp.call\'&&<button className="primary-button"').replace('Permitir na sessão</button><button','Permitir na sessão</button>}<button');
s=s.replace("<button className={'nav-item '+(page==='skills'","<button className={'nav-item '+(page==='mcp'?'selected':'')} onClick={()=>navigate('mcp')}><Plug size={22}/>Servidores MCP</button><button className={'nav-item '+(page==='context'?'selected':'')} onClick={()=>navigate('context')}><Network size={22}/>Contexto</button><button className={'nav-item '+(page==='skills'");
s=s.replace("skills:'Skills',settings:","mcp:'Servidores MCP',context:'Contexto do projeto',skills:'Skills',settings:");
s=s.replace("skills:'Inspeção dos arquivos SKILL.md do projeto.',settings:","mcp:'Conecte ferramentas e acompanhe suas permissões.',context:'Arquivos, símbolos e referências com origem explícita.',skills:'Crie e inspecione orientações reutilizáveis do projeto.',settings:");
const start=s.indexOf(" {page==='skills'&&("),end=s.indexOf(" {page==='settings'&&",start);
if(start<0||end<0)throw Error('Skill section not found');
s=s.slice(0,start)+" {page==='mcp'&&<McpPanel offline={offline} onError={fail}/>}\n {page==='context'&&<ContextPanel workspace={workspace} onOpen={path=>{setPage('ide');openFile(path)}} onError={fail}/>}\n {page==='skills'&&<SkillsPanel workspace={workspace} onError={fail}/>}\n"+s.slice(end);
s=s.replace('<IconButton label="Modelos" onClick={()=>setModal(\'provider\')}><Box size={22}/></IconButton>','<IconButton label="Modelos" onClick={()=>setModal(\'provider\')}><Box size={22}/></IconButton><IconButton label="Contexto do projeto" onClick={()=>navigate(\'context\')}><Network size={22}/></IconButton><IconButton label="Servidores MCP" onClick={()=>navigate(\'mcp\')}><Plug size={22}/></IconButton>');
return s;
});
