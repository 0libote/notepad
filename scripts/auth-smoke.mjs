import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {writeFile,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {startTestServer,signIn,password} from './test-server.mjs';
const fixture=await startTestServer(3139);const {url,data}=fixture;
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox']});
const disk=page=>page.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result;const r=db.transaction('drafts').objectStore('drafts').get('workspace');r.onsuccess=()=>{db.close();resolve(r.result?JSON.parse(r.result):null);};};}));
const saved=page=>page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
const edit=(page,value)=>page.$eval('.document',(el,v)=>{el.value=v;el.dispatchEvent(new Event('input',{bubbles:true}));},value);
try {
 const unauth=await fetch(url+'/api/workspace');assert.equal(unauth.status,401);assert.equal(unauth.headers.get('cache-control'),'no-store');
 const denied=await fetch(url+'/api/workspace',{method:'PUT',headers:{...fixture.headers,'content-type':'application/json',origin:'https://attacker.example'},body:'{}'});assert.equal(denied.status,403);
 const context=await browser.createBrowserContext();const page=await context.newPage();const errors=[];await page.evaluateOnNewDocument(()=>{
  window.privateReads=[];const original=window.fetch;
  window.fetch=function(input,init){const request=input instanceof Request?input:new Request(input,init);if(request.method==='GET'&&(request.url.endsWith('/api/workspace')||request.url.endsWith('/api/auth/state')))window.privateReads.push(request.cache);return original.apply(this,arguments);};
 });page.on('pageerror',e=>errors.push(e.message));
 await page.setViewport({width:1440,height:900});await page.goto(url+'/#write',{waitUntil:'networkidle0'});await page.waitForSelector('#owner-password');await page.screenshot({path:'/tmp/notepad-signin.png'});await page.setViewport({width:375,height:812});await page.screenshot({path:'/tmp/notepad-signin-mobile.png'});assert.ok(await page.$eval('.status',e=>e.getBoundingClientRect().height>0),'Persistence status must remain visible on mobile');await page.setViewport({width:1440,height:900});
 assert.equal(await page.$('.document'),null);assert.equal(await disk(page),null);
 await page.type('#owner-password','wrong-password');await page.click('.auth-fields button');await page.waitForFunction(()=>document.querySelector('.auth-error')?.textContent.includes('Password not accepted'));
 assert.equal(await page.$eval('#owner-password',el=>el.value),'');assert.equal(await disk(page),null);
 await signIn(page);await saved(page);assert.ok(await page.evaluate(()=>window.privateReads.length>0&&window.privateReads.every(v=>v==='no-store')),'Private reads must bypass old HTTP caches');
 const cookies=await context.cookies();assert.ok(cookies.find(c=>c.name==='notepad_session')?.httpOnly);
 assert.equal(await page.evaluate(()=>document.cookie.includes('notepad_session')),false);
 assert.equal(await page.evaluate(()=>localStorage.length),0);
 await edit(page,'Draft kept when the session expires');await saved(page);
 // Resetting the owner password invalidates existing sessions while preserving notes.
 const path=join(data,'reset-password');await writeFile(path,password,{mode:0o600});
 await promisify(execFile)('target/debug/notepad-server',['setup','--password-file',path],{env:{...process.env,NOTEPAD_DATA:data}});await rm(path);
 await page.click('.topbar .primary');await page.waitForSelector('#owner-password');
 assert.equal(await page.$eval('.document',el=>el.value),'Draft kept when the session expires');assert.equal((await disk(page)).notes[0].body,'Draft kept when the session expires');
 await signIn(page);assert.equal(await page.$eval('.document',el=>el.value),'Draft kept when the session expires');
 await page.click('.topbar .primary');await page.waitForFunction(()=>document.querySelector('.status')?.textContent.startsWith('Synced with server'));await saved(page);
 // A second tab shares the same session and draft. Sign-out must clear both views.
 const second=await context.newPage();await second.goto(url+'/#write',{waitUntil:'networkidle0'});await second.waitForFunction(()=>document.querySelector('.locked-workspace')?.textContent.includes('another tab'));
 await page.bringToFront();await edit(page,'Unsynced content removed on explicit sign-out');await saved(page);
 page.on('dialog',dialog=>dialog.accept());await page.click('.session-control:not(.hidden-control)');
 await page.waitForSelector('#owner-password');await page.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Browser copy removed'));await second.bringToFront();await second.waitForSelector('.locked-workspace');
 await second.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Browser copy removed'));
 assert.equal(await disk(page),null);assert.equal(await disk(second),null);
 assert.equal(await second.$('.document'),null);
 assert.equal(await page.evaluate(async()=> (await fetch('/api/workspace')).status),401);
 await page.bringToFront();await signIn(page);await saved(page);assert.equal(await page.$eval('.document',el=>el.value),'Draft kept when the session expires');
 assert.deepEqual(errors,[]);await page.screenshot({path:'/tmp/notepad-signed-in.png'});
 console.log('PASS: authenticated startup, wrong-password handling, HttpOnly session, reset/revocation, unsynced draft recovery, server save, cross-tab sign-out and local removal');
} finally {await browser.close();await fixture.stop();}
