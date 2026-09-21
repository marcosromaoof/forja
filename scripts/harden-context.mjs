import fs from 'node:fs';
function edit(p,f){fs.writeFileSync(p,f(fs.readFileSync(p,'utf8')))}
edit('crates/core/src/context.rs',s=>s.replace('node.named_children(&mut c).find_map(|n|n.child_by_field_name("name"))','let found=node.named_children(&mut c).find_map(|n|n.child_by_field_name("name"));found'));
edit('crates/core/src/policy.rs',s=>s.replace('let n=s.to_string_lossy().to_lowercase();','let n=s.to_string_lossy().to_lowercase();let stem=n.split(\'.\').next().unwrap_or("");ensure!(!["con","prn","aux","nul","com1","com2","com3","com4","com5","com6","com7","com8","com9","lpt1","lpt2","lpt3","lpt4","lpt5","lpt6","lpt7","lpt8","lpt9","conin$","conout$"].contains(&stem),"Dispositivo reservado do Windows");').replace('"foo."]','"foo.","NUL.txt","aux","COM1.log","dir/LPT9"]'));
