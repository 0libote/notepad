// Run only against a disposable deployment configured with the test-only fixture password.
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import https from 'node:https';
import {password} from './test-server.mjs';
const url=process.env.NOTEPAD_TEST_URL;
if(!url||!url.startsWith('https://'))throw Error('Set NOTEPAD_TEST_URL to a disposable HTTPS deployment.');
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox','--ignore-certificate-errors']});
const context=await browser.createBrowserContext(),page=await context.newPage(),errors=[];
page.on('pageerror',e=>errors.push(e.message));
const saved=()=>page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
async function sync(){await page.waitForFunction(()=>!document.querySelector('.topbar .primary')?.disabled);await page.click('.topbar .primary');await page.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));}
try{
 await page.goto(url+'/#canvas',{waitUntil:'networkidle0'});
 assert.equal(await page.evaluate(()=>isSecureContext&&!!navigator.locks),true);
 await page.waitForSelector('#owner-password');await page.type('#owner-password',password);await page.click('.auth-fields .primary');await page.waitForSelector('#canvas-card-idea');await saved();
 const cookies=await context.cookies();assert.ok(cookies.some(c=>c.name==='__Host-notepad_session'&&c.secure&&c.httpOnly));assert.equal(await page.evaluate(()=>document.cookie.includes('notepad_session')),false);
 await page.click('[aria-label="Connect cards"]');await page.click('#canvas-card-idea .card-handle');await page.click('#canvas-card-sticky .card-handle');await page.waitForSelector('.connection');
 await page.$eval('#canvas-card-idea .card-handle',e=>e.focus());await page.$eval('#canvas-card-idea .resize-handle',e=>e.focus());await page.keyboard.press('ArrowRight');await sync();await saved();
 let remote=await page.evaluate(async()=>{const r=await fetch('/api/workspace',{cache:'no-store'});if(!r.ok)throw Error('Remote workspace unavailable');return r.json();});assert.equal(remote.notes[0].connections.length,1);assert.ok(remote.notes[0].cards[0].size.width>300);
 await page.reload({waitUntil:'networkidle0'});await page.waitForSelector('.connection');await saved();
 // An interrupted network leaves a usable local draft, then resumes on the real server.
 let offline=true;await page.setRequestInterception(true);page.on('request',r=>offline&&r.url().includes('/api/')?r.abort():r.continue());
 await page.click('.view-tabs button:first-child');await page.$eval('.document',e=>{e.value='HomeServer offline recovery verified';e.dispatchEvent(new Event('input',{bubbles:true}));});await saved();
 await page.reload({waitUntil:'networkidle0'});await saved();assert.equal(await page.$eval('.document',e=>e.value),'HomeServer offline recovery verified');
 offline=false;await sync();remote=await page.evaluate(async()=> (await fetch('/api/workspace',{cache:'no-store'})).json());assert.equal(remote.notes[0].body,'HomeServer offline recovery verified');
 const session=(await context.cookies()).find(c=>c.name==='__Host-notepad_session');const denied=await new Promise((resolve,reject)=>{const request=https.request(new URL('/api/workspace',url),{method:'PUT',rejectUnauthorized:false,headers:{origin:'https://attacker.invalid',cookie:session.name+'='+session.value,'content-type':'application/json'}},response=>{response.resume();resolve(response.statusCode);});request.on('error',reject);request.end('{}');});assert.equal(denied,403);
 await page.setViewport({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
 assert.deepEqual(errors,[]);console.log('PASS: remote Docker HTTPS, secure cookies, Web Locks, canvas editing, server save, reload, offline recovery, mutation guard and mobile shell');
}finally{await browser.close();}
