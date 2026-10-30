import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {startTestServer,signIn} from './test-server.mjs';
const fixture=await startTestServer(3141),{url,headers}=fixture;
const server=async()=>{const r=await fetch(url+'/api/workspace',{headers});assert.equal(r.status,200);return r.json();};
let seed=await server();seed.notes=Array.from({length:3},(_,i)=>({...structuredClone(seed.notes[0]),id:'page-'+i,title:['Alpha','Beta','Gamma'][i],body:'Initial '+i,cards:[],strokes:[]}));
assert.equal((await fetch(url+'/api/workspace',{method:'PUT',headers:{...headers,'content-type':'application/json'},body:JSON.stringify(seed)})).status,200);
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox','--disable-background-timer-throttling','--disable-renderer-backgrounding']});
const errors=[];
const disk=p=>p.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result,tx=db.transaction('drafts'),s=tx.objectStore('drafts'),w=s.get('workspace'),b=s.get('sync-base');tx.oncomplete=()=>{db.close();resolve({workspace:w.result&&JSON.parse(w.result),base:b.result&&JSON.parse(b.result)});};};}));
const saved=p=>p.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
const edit=(p,text)=>p.$eval('.document',(el,text)=>{el.value=text;el.dispatchEvent(new Event('input',{bubbles:true}));},text);
const text=p=>p.$eval('.document',el=>el.value);
const open=async(p,id)=>{await p.bringToFront();await p.evaluate(id=>{location.hash=`note=${id}&view=write`;},id);await p.waitForFunction(id=>new URLSearchParams(location.hash.slice(1)).get("note")===id&&document.querySelector('.note-identity input')?.value===({ 'page-0':'Alpha','page-1':'Beta','page-2':'Gamma'}[id]),{},id);};
const waitServer=async test=>{for(let i=0;i<100;i++){const w=await server();if(test(w))return w;await new Promise(r=>setTimeout(r,150));}throw Error('Server did not reach expected state: '+JSON.stringify((await server()).notes.map(n=>[n.title,n.body])));};
async function device(){const context=await browser.createBrowserContext(),page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));let offline=false,hold=false,held=null,lose=false;
 await page.setRequestInterception(true);page.on('request',async r=>{
  if(!r.url().includes('/api/'))return r.continue();
  if(offline)return r.abort();
  if(r.method()==='PUT'&&hold){held=r;return;}
  if(r.method()==='PUT'&&lose){lose=false;await fetch(r.url(),{method:'PUT',headers:r.headers(),body:r.postData()});return r.abort();}
  return r.continue();
 });
 await page.goto(url+'/#note=page-0&view=write',{waitUntil:'networkidle0'});await signIn(page);await saved(page);
 return {page,context,offline(v){offline=v;},hold(v){hold=v;},lose(){lose=true;},async release(){const r=held;held=null;hold=false;assert.ok(r);await r.continue();},async waitHeld(){for(let i=0;i<100&&!held;i++)await new Promise(r=>setTimeout(r,50));assert.ok(held,'PUT request held');}};
}
async function sync(p){await p.bringToFront();await p.waitForFunction(()=>!document.querySelector('.topbar .primary')?.disabled);await p.click('.topbar .primary');}
try{
 const a=await device(),b=await device();const ap=a.page,bp=b.page;
 a.offline(true);await edit(ap,'Alpha written offline');await saved(ap);
 await open(bp,'page-1');await edit(bp,'Beta from second device');await waitServer(w=>w.notes[1].body==='Beta from second device');
 await ap.reload({waitUntil:'networkidle0'});await saved(ap);assert.equal(await text(ap),'Alpha written offline');
 const retained=await disk(ap);assert.equal(retained.base.revision,1);assert.equal(retained.base.notes[0].body,'Initial 0');
 a.offline(false);await sync(ap);
 const combined=await waitServer(w=>w.notes[0].body==='Alpha written offline'&&w.notes[1].body==='Beta from second device');
 await saved(ap);assert.equal((await disk(ap)).base.revision,combined.revision);assert.equal(await ap.$('.sync-conflict'),null);
 await sync(bp);await bp.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));
 await open(bp,'page-0');assert.equal(await text(bp),'Alpha written offline');
 console.log('PASS: offline reload retains baseline; different page edits merge automatically');
 // One page changed in both profiles: neither local nor server text may be discarded.
 a.offline(true);b.offline(true);await edit(ap,'Alpha from offline device A');await edit(bp,'Alpha from device B');await saved(ap);await saved(bp);
 b.offline(false);await sync(bp);await waitServer(w=>w.notes[0].body==='Alpha from device B');
 a.offline(false);await sync(ap);await ap.waitForSelector('.sync-conflict');
 assert.equal(await text(ap),'Alpha from offline device A');assert.equal((await server()).notes.length,3);
 await ap.setViewport({width:390,height:844});await ap.screenshot({path:'/tmp/notepad-sync-conflict-mobile.png'});
 assert.equal(await ap.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
 // Conflict status is recomputed after a reload using the durable original baseline.
 await ap.reload({waitUntil:'networkidle0'});await ap.waitForSelector('.sync-conflict');await ap.click('.sync-conflict .primary');
 const recovered=await waitServer(w=>w.notes.some(n=>n.body==='Alpha from offline device A')&&w.notes.some(n=>n.body==='Alpha from device B'));
 assert.equal(recovered.notes.length,4);assert.equal(new Set(recovered.notes.map(n=>n.id)).size,4);assert.ok(recovered.notes.some(n=>n.title==='Alpha (device copy)'));
 await ap.waitForFunction(()=>!document.querySelector('.sync-conflict'));await saved(ap);await ap.setViewport({width:1280,height:850});
 console.log('PASS: same-page conflict survives reload; Keep both preserves every page');
 // Edits during an in-flight upload remain local and are sent in a subsequent request.
 await open(ap,'page-2');a.hold(true);await edit(ap,'Gamma sent first');await a.waitHeld();await edit(ap,'Gamma typed while uploading');await saved(ap);await a.release();
 await waitServer(w=>w.notes.find(n=>n.id==='page-2').body==='Gamma typed while uploading');assert.equal(await text(ap),'Gamma typed while uploading');
 console.log('PASS: typing during upload survives the response and syncs afterward');
 // A committed request whose response is lost is safe to retry without recovery duplicates.
 a.lose();await edit(ap,'Gamma committed but response lost');await waitServer(w=>w.notes.find(n=>n.id==='page-2').body==='Gamma committed but response lost');
 await sync(ap);await ap.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));await saved(ap);
 assert.equal((await server()).notes.length,4);assert.equal((await disk(ap)).workspace.revision,(await server()).revision);
 // Idle polling receives remote work without a button press or a local edit.
 await sync(bp);await open(bp,'page-1');await edit(bp,'Beta received by idle poll');await waitServer(w=>w.notes.find(n=>n.id==='page-1').body==='Beta received by idle poll');
 await open(ap,'page-1');await ap.waitForFunction(()=>document.querySelector('.document')?.value==='Beta received by idle poll',{timeout:22000});
 assert.deepEqual(errors,[]);console.log('PASS: lost-response retry is idempotent; idle clients receive server changes');
 // Both devices read one revision before either PUT can finish. The loser retries and merges.
 await open(ap,'page-2');await open(bp,'page-0');a.hold(true);b.hold(true);
 await edit(ap,'Gamma race from A');await edit(bp,'Alpha race from B');await a.waitHeld();await b.waitHeld();
 await a.release();await waitServer(w=>w.notes.find(n=>n.id==='page-2').body==='Gamma race from A');await b.release();
 await sync(bp);await waitServer(w=>w.notes.find(n=>n.id==='page-0').body==='Alpha race from B'&&w.notes.find(n=>n.id==='page-2').body==='Gamma race from A');
 assert.equal((await server()).notes.length,4);console.log('PASS: simultaneous PUT revisions reject a stale write and merge on retry');
 // A failure writing the baseline must roll back the preceding workspace write too.
 await sync(ap);await ap.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));await saved(ap);
 const before=await disk(ap);
 await ap.evaluate(()=>{window.failBaseline=true;const put=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(value,key){if(window.failBaseline&&key==='sync-base')throw new DOMException('Simulated baseline quota failure','QuotaExceededError');return put.call(this,value,key);};});
 await edit(ap,'Gamma only in memory');await ap.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='error');
 await sync(ap);await ap.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('local save failed'));
 assert.deepEqual(await disk(ap),before);assert.equal((await server()).notes.find(n=>n.id==='page-2').body,'Gamma race from A');
 await ap.evaluate(()=>window.failBaseline=false);await edit(ap,'Gamma durable again');await waitServer(w=>w.notes.find(n=>n.id==='page-2').body==='Gamma durable again');
 console.log('PASS: baseline-write failure rolls back the draft and blocks upload; retry recovers');
 const duplicate=await a.context.newPage();await duplicate.goto(url+'/#write',{waitUntil:'networkidle0'});
 await duplicate.waitForFunction(()=>document.querySelector('.locked-workspace')?.textContent.includes('another tab'));
 assert.equal(await duplicate.$('.document'),null);assert.equal(await duplicate.$eval('[aria-label="Import backup"]',el=>el.disabled),true);
 await ap.close();await duplicate.reload({waitUntil:'networkidle0'});await duplicate.waitForSelector('.document');await saved(duplicate);
 assert.ok((await disk(duplicate)).workspace.notes.some(n=>n.body==='Alpha from offline device A'));
 console.log('PASS: one editing tab protects the shared cache; closing it releases the lock');
}catch(error){for(const p of await browser.pages()){try{console.error(await p.evaluate(()=>({url:location.href,title:document.querySelector('[aria-label="Note title"]')?.value,status:document.querySelector('.status')?.textContent,notes:[...document.querySelectorAll('.note-row span')].map(e=>e.textContent)})));}catch{}}throw error;}finally{await browser.close();await fixture.stop();}
