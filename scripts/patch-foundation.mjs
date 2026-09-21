import fs from 'node:fs';
function edit(p,fn){fs.writeFileSync(p,fn(fs.readFileSync(p,'utf8')))}
edit('apps/desktop/index.html',s=>s.replace('<title>','<link rel="icon" type="image/svg+xml" href="/logo.svg"/><title>'));
fs.mkdirSync('apps/desktop/public',{recursive:true});fs.copyFileSync('apps/desktop/logo.svg','apps/desktop/public/logo.svg');
edit('apps/desktop/src/CodePane.tsx',s=>s.replace("import Editor,","import {useRef} from 'react';\nimport Editor,").replace(" return <Editor path={path}"," const save=useRef(onSave);save.current=onSave;\n return <Editor path={path}").replace("monaco.KeyCode.KeyS,onSave","monaco.KeyCode.KeyS,()=>save.current()"));
edit('apps/desktop/src/App.tsx',s=>s.replace("onChange={v=>setEdited(old=>({...old,[activeFile.path]:v}))}","onChange={v=>setEdited(old=>{const n={...old};if(v===activeFile.content)delete n[activeFile.path];else n[activeFile.path]=v;return n})}"));
edit('crates/core/src/files.rs',s=>s.replace('ensure!(!old.is_empty()&&before.content.matches(old).count()==1,','ensure!((old.is_empty()&&before.content.is_empty())||(!old.is_empty()&&before.content.matches(old).count()==1),'));
edit('.gitignore',s=>s+'\n.playwright-mcp/\nforja-*.png\n*.tsbuildinfo\n');
edit('apps/desktop/src/styles.css',s=>s+`
@media(min-width:1500px){.home-sidebar{width:290px}.brand{min-width:241px}.nav-item{font-size:16px;min-height:49px}.nav-section-title{font-size:12px}.new-project{font-size:17px;min-height:56px}.welcome-content{width:min(1160px,calc(100% - 100px))}.welcome-content>h1{font-size:44px}.welcome-subtitle{font-size:22px}.quick-actions{width:86%;gap:22px}.quick-actions button{font-size:17px;padding:25px 22px}.composer textarea{font-size:20px;height:106px}.composer-left>.icon-button{width:48px;height:48px}.mode-toggle button{font-size:13px;padding:11px 16px}.model-button{min-width:195px;font-size:13px;padding:8px 15px}.model-button small{font-size:12px}.send-button{width:56px;height:56px}.trust-chips>span,.trust-chips>button{font-size:13px;padding:11px 20px}}
`);
