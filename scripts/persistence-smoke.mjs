import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {spawn} from 'node:child_process';
import {mkdtemp,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {startTestServer} from './test-server.mjs';
const fixture=await startTestServer(3138);const {data,url}=fixture;
const seed=await (await fetch(url+'/api/workspace',{headers:fixture.headers})).json();
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox']});
const disk=page=>page.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result;if(!db.objectStoreNames.contains('drafts')){db.close();resolve(null);return;}const r=db.transaction('drafts').objectStore('drafts').get('workspace');r.onsuccess=()=>{db.close();resolve(r.result?JSON.parse(r.result):null);};};}));
const saved=p=>p.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
const edit=(p,text)=>p.$eval('.document',(el,text)=>{el.value=text;el.dispatchEvent(new Event('input',{bubbles:true}));},text);
try {
 const context=await browser.createBrowserContext();const page=await context.newPage();
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.evaluateOnNewDocument(w=>{
  if(!sessionStorage.getItem('seeded')){w.schema=1;w.notes[0].body='My legacy draft';localStorage.setItem('notepad.workspace.v1',JSON.stringify(w));sessionStorage.setItem('seeded','yes');}
  window.draftWrites=0;const put=IDBObjectStore.prototype.put;
  IDBObjectStore.prototype.put=function(...args){if(window.failWrites)throw new DOMException('Simulated quota exhaustion','QuotaExceededError');window.draftWrites++;return put.apply(this,args);};
 },seed);
 await page.goto(url+'/#write',{waitUntil:'networkidle0'});await saved(page);
 assert.equal((await disk(page)).notes[0].body,'My legacy draft');assert.equal((await disk(page)).schema,seed.schema);
 assert.equal(await page.evaluate(()=>localStorage.getItem('notepad.workspace.v1')),null);
 const writes=await page.evaluate(()=>window.draftWrites);
 await page.$eval('.document',el=>{for(let i=0;i<100;i++){el.value='Rapid edit '+i;el.dispatchEvent(new Event('input',{bubbles:true}));}});await saved(page);
 assert.equal((await disk(page)).notes[0].body,'Rapid edit 99');
 assert.ok((await page.evaluate(()=>window.draftWrites))-writes<=2,'Coalesce edits instead of writing every event');
 // The host API may be offline; local drafts should still open and accept edits.
 await page.setRequestInterception(true);page.on('request',r=>r.url().includes('/api/')?r.abort():r.continue());
 await page.reload({waitUntil:'networkidle0'});await saved(page);assert.equal(await page.$eval('.document',e=>e.value),'Rapid edit 99');
 await edit(page,'Offline draft');await saved(page);assert.equal((await disk(page)).notes[0].body,'Offline draft');
 // Simulated failed commit must report an error and leave the last good draft intact.
 await page.evaluate(()=>window.failWrites=true);await edit(page,'Not on disk yet');
 await page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='error');
 assert.ok(await page.$eval('.status',e=>e.textContent.includes('Export a backup')));
 assert.equal((await disk(page)).notes[0].body,'Offline draft');
 await page.evaluate(()=>window.failWrites=false);await edit(page,'Retry recovered');await saved(page);assert.equal((await disk(page)).notes[0].body,'Retry recovered');
 // A valid workspace larger than typical localStorage quotas survives import and reload.
 const big=structuredClone(seed);big.notes=Array.from({length:7},(_,i)=>({...structuredClone(seed.notes[0]),id:'large-'+i,title:'Large note '+i,body:'x'.repeat(900000)}));
 const backup=join(data,'large.json');await writeFile(backup,JSON.stringify(big));page.on('dialog',d=>d.accept());
 await (await page.$('[aria-label="Import backup"]')).uploadFile(backup);await saved(page);
 await page.waitForFunction(()=>document.querySelectorAll('.note-row').length===7);await saved(page);
 assert.equal((await disk(page)).notes.length,7);await page.reload({waitUntil:'networkidle0'});await saved(page);assert.equal((await disk(page)).notes[6].body.length,900000);
 assert.deepEqual(errors,[]);await context.close();
 // Existing IndexedDB drafts and their baseline upgrade together on the next local commit.
 const migration=await browser.createBrowserContext();const mp=await migration.newPage();await mp.goto(url+'/api/health');
 await mp.evaluate(w=>new Promise((resolve,reject)=>{w.schema=1;const base=structuredClone(w);w.notes[0].body='Version one offline draft';const open=indexedDB.open('notepad-local',1);open.onupgradeneeded=()=>open.result.createObjectStore('drafts');open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result,tx=db.transaction('drafts','readwrite'),store=tx.objectStore('drafts');store.put(JSON.stringify(w),'workspace');store.put(JSON.stringify(base),'sync-base');tx.oncomplete=()=>{db.close();resolve();};tx.onerror=()=>reject(tx.error);};}),seed);
 await mp.goto(url+'/#write',{waitUntil:'networkidle0'});await saved(mp);assert.equal(await mp.$eval('.document',e=>e.value),'Version one offline draft');await edit(mp,'Upgraded offline draft');await saved(mp);assert.equal((await disk(mp)).schema,seed.schema);
 const upgradedBase=await mp.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result,r=db.transaction('drafts').objectStore('drafts').get('sync-base');r.onsuccess=()=>{db.close();resolve(JSON.parse(r.result));};};}));assert.equal(upgradedBase.schema,seed.schema);assert.equal(upgradedBase.revision,seed.revision);assert.equal(upgradedBase.notes[0].body,seed.notes[0].body);await migration.close();
 // A browser without Web Locks cannot become a second uncoordinated cache writer.
 const unsupported=await browser.createBrowserContext();const up=await unsupported.newPage();
 await up.evaluateOnNewDocument(()=>Object.defineProperty(navigator,'locks',{value:undefined}));
 await up.goto(url+'/#write',{waitUntil:'networkidle0'});
 await up.waitForFunction(()=>document.querySelector('.locked-workspace')?.textContent.includes('Web Locks'));
 assert.equal(await up.$('.document'),null);assert.equal(await disk(up),null);await unsupported.close();
 // A corrupt legacy draft must remain recoverable and must not be replaced with defaults.
 const corrupt=await browser.createBrowserContext();const cp=await corrupt.newPage();
 await cp.evaluateOnNewDocument(()=>localStorage.setItem('notepad.workspace.v1','{broken'));
 await cp.goto(url+'/#write',{waitUntil:'networkidle0'});await cp.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('could not be read'));
 assert.equal(await cp.$('.document'),null);assert.equal(await cp.evaluate(()=>localStorage.getItem('notepad.workspace.v1')),'{broken');assert.equal(await disk(cp),null);await corrupt.close();
 // Exercise the packaged UI's IPC path without claiming this substitutes for native WebView testing.
 const desktop=await browser.createBrowserContext();const dp=await desktop.newPage();const network=[];dp.on('request',r=>{if(r.url().includes('/api/'))network.push(r.url());});
 await dp.evaluateOnNewDocument(w=>{
  window.activeWrites=0;window.maxWrites=0;
  window.__TAURI__={core:{invoke:async(command,args)=>{
   if(command==='connection')return null;
   if(command==='auth_state')return {configured:true,authenticated:true};
   if(command==='load_sync_base')return null;
   if(command==='load_draft')return JSON.parse(localStorage.getItem('native-fixture')||'null');
   if(command==='load_server')return w;
   if(command==='save_server')return args.workspace;
   if(command==='save_draft'){
    window.activeWrites++;window.maxWrites=Math.max(window.maxWrites,window.activeWrites);
    await new Promise(r=>setTimeout(r,350));localStorage.setItem('native-fixture',JSON.stringify(args.workspace));window.activeWrites--;return null;
   }
   throw Error('Unexpected command '+command);
  }}};
 },seed);
 await dp.goto(url+'/#write',{waitUntil:'domcontentloaded'});await dp.waitForFunction(()=>window.activeWrites===1);
 await edit(dp,'Changed during an in-flight draft save');await saved(dp);
 assert.equal(await dp.evaluate(()=>JSON.parse(localStorage.getItem('native-fixture')).notes[0].body),'Changed during an in-flight draft save');
 assert.equal(await dp.evaluate(()=>window.maxWrites),1);assert.deepEqual(network,[]);
 assert.equal(await dp.$eval('.topbar .primary',e=>e.textContent),'Save locally');await desktop.close();
 console.log('PASS: legacy and format migration, coalesced writes, offline API, failed-write recovery, drafts over 5 MB, corrupt-copy protection, ordered desktop IPC saves');
} finally {await browser.close();await fixture.stop();}
