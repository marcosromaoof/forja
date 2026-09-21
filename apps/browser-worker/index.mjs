import {createInterface} from 'node:readline';
import {chromium} from 'playwright';
import {existsSync,readdirSync} from 'node:fs';
import {join} from 'node:path';

let browser, context, page;
let allowedOrigins=[];
const consoleEntries=[];
const networkEntries=[];
const limit=(values,item)=>{values.push(item);if(values.length>500)values.shift()};
const originOf=value=>new URL(value).origin;
function installedBrowser(){
 const explicit=process.env.FORJA_CHROMIUM;
 const candidates=[explicit,
  process.env.LOCALAPPDATA&&join(process.env.LOCALAPPDATA,'Google','Chrome','Application','chrome.exe'),
  process.env['PROGRAMFILES(X86)']&&join(process.env['PROGRAMFILES(X86)'],'Microsoft','Edge','Application','msedge.exe'),
  process.env.PROGRAMFILES&&join(process.env.PROGRAMFILES,'Microsoft','Edge','Application','msedge.exe')].filter(Boolean);
 const cache=process.env.LOCALAPPDATA&&join(process.env.LOCALAPPDATA,'ms-playwright');
 if(cache&&existsSync(cache))for(const directory of readdirSync(cache).filter(name=>name.startsWith('chromium-')).sort().reverse())for(const relative of ['chrome-win64/chrome.exe','chrome-win/chrome.exe'])candidates.unshift(join(cache,directory,relative));
 return candidates.find(path=>existsSync(path));
}
function allowed(url){const origin=originOf(url);if(!allowedOrigins.includes(origin))throw new Error(`Origem não autorizada: ${origin}`)}
async function ensurePage(){if(!page)throw new Error('Contexto do navegador não iniciado')}
async function start(args){
 if(browser)await browser.close();
 allowedOrigins=(args.origins??[]).map(originOf);
 if(!allowedOrigins.length)throw new Error('Informe ao menos uma origem autorizada');
 const executablePath=installedBrowser();
 browser=await chromium.launch({headless:true,...(executablePath?{executablePath}:{})});
 context=await browser.newContext({acceptDownloads:false,serviceWorkers:'block'});
 page=await context.newPage();
 page.on('console',message=>limit(consoleEntries,{type:message.type(),text:message.text(),timestamp:new Date().toISOString()}));
 page.on('request',request=>limit(networkEntries,{phase:'request',method:request.method(),url:request.url(),resourceType:request.resourceType()}));
 page.on('response',response=>limit(networkEntries,{phase:'response',status:response.status(),url:response.url()}));
 page.on('download',download=>download.cancel());
 page.on('dialog',dialog=>dialog.dismiss());
 return {started:true,origins:allowedOrigins};
}
async function command(name,args){
 if(name==='start')return start(args);
 await ensurePage();
 if(name==='navigate'){allowed(args.url);await page.goto(args.url,{waitUntil:'domcontentloaded',timeout:args.timeout_ms??30000});return {url:page.url(),title:await page.title()}}
 if(name==='snapshot')return {url:page.url(),title:await page.title(),aria:await page.locator('body').ariaSnapshot({timeout:10000})};
 if(name==='find'){const locator=args.selector?page.locator(args.selector):page.getByText(args.text,{exact:!!args.exact});const count=await locator.count();return {count,items:await Promise.all(Array.from({length:Math.min(count,50)},async(_,index)=>{const item=locator.nth(index);return {text:(await item.innerText({timeout:3000}).catch(()=>'' )).slice(0,500),visible:await item.isVisible().catch(()=>false)}}))}}
 if(name==='click'){const locator=args.selector?page.locator(args.selector):page.getByText(args.text,{exact:!!args.exact});await locator.nth(args.index??0).click({timeout:args.timeout_ms??10000});return {url:page.url()}}
 if(name==='type'){const locator=page.locator(args.selector);if(args.clear!==false)await locator.fill('');await locator.type(args.text,{delay:args.delay_ms??0});return {typed:true}}
 if(name==='select'){await page.locator(args.selector).selectOption(args.values);return {selected:true}}
 if(name==='press'){await page.locator(args.selector??'body').press(args.key);return {pressed:true}}
 if(name==='wait'){if(args.selector)await page.locator(args.selector).waitFor({state:args.state??'visible',timeout:args.timeout_ms??10000});else await page.waitForTimeout(Math.min(args.timeout_ms??1000,30000));return {waited:true}}
 if(name==='screenshot'){const buffer=await page.screenshot({fullPage:args.full_page!==false,type:'png'});return {url:page.url(),title:await page.title(),media_type:'image/png',base64:buffer.toString('base64'),bytes:buffer.length}}
 if(name==='console')return {entries:consoleEntries.splice(0)};
 if(name==='network')return {entries:networkEntries.splice(0)};
 if(name==='close'){await browser?.close();browser=context=page=undefined;return {closed:true}}
 throw new Error(`Comando desconhecido: ${name}`);
}

const input=createInterface({input:process.stdin,crlfDelay:Infinity});
for await(const line of input){
 let request;
 try{request=JSON.parse(line);const result=await command(request.command,request.args??{});process.stdout.write(JSON.stringify({id:request.id,ok:true,result})+'\n')}
 catch(error){process.stdout.write(JSON.stringify({id:request?.id,ok:false,error:error instanceof Error?error.message:String(error)})+'\n')}
}
await browser?.close();
