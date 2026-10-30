import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {spawn} from 'node:child_process';
import {mkdtemp, rm, readFile, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {startTestServer,signIn} from './test-server.mjs';
const fixture=await startTestServer(3137);const {data,url:base}=fixture;

const browser = await puppeteer.launch({executablePath:process.env.CHROME_PATH || '/usr/bin/chromium',headless:true,args:['--no-sandbox']});
const context=await browser.createBrowserContext();
const page=await context.newPage();const errors=[];const remoteRequests=[];page.on('request',r=>{if(r.url().includes('tracking.invalid'))remoteRequests.push(r.url());});page.on('pageerror',e=>errors.push(e.message));

const read=async()=>{
 await page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
 return page.evaluate(()=>new Promise((resolve,reject)=>{
  const open=indexedDB.open('notepad-local',1);
  open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result;const req=db.transaction('drafts').objectStore('drafts').get('workspace');req.onsuccess=()=>{db.close();resolve(JSON.parse(req.result));};req.onerror=()=>reject(req.error);};
 }));
};
try {
 await page.setViewport({width:1440,height:900});await page.goto(base,{waitUntil:'networkidle0'});await signIn(page);
 await page.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('browser'));
 await page.evaluate(()=>document.fonts.ready);
 assert.ok(await page.evaluate(()=>[...document.fonts].some(f=>f.family==='Figtree' && f.status==='loaded')),'Bundled fonts must load');
 await page.screenshot({path:'/tmp/notepad-desktop.png'});
 assert.equal(await page.$$eval('.canvas-card',els=>els.length),3);
 await page.click('[aria-label="Add text card"]');await page.waitForFunction(()=>document.querySelectorAll('.canvas-card').length===4);
 const before=(await read()).notes[0].cards[0];
 const handle=await page.$('.canvas-card .card-handle');const box=await handle.boundingBox();
 await page.mouse.move(box.x+60,box.y+15);await page.mouse.down();await page.mouse.move(box.x+140,box.y+65,{steps:5});await page.mouse.up();
 const after=(await read()).notes[0].cards[0];assert.equal(after.x,before.x+80);assert.equal(after.y,before.y+50);
 await page.click('[aria-label="Undo"]');assert.equal((await read()).notes[0].cards[0].x,before.x);
 await (await page.$('.canvas-card .card-handle')).focus();await page.keyboard.press('ArrowRight');
 assert.equal((await read()).notes[0].cards[0].x,before.x+10);
 await page.click('[aria-label="Undo"]');
 await page.click('[aria-label="Draw"]');const world=await page.$('#world');const wb=await world.boundingBox();
 await page.mouse.move(wb.x+830,wb.y+140);await page.mouse.down();await page.mouse.move(wb.x+900,wb.y+200,{steps:5});await page.mouse.up();
 assert.ok((await read()).notes[0].strokes[0].length>1);
 await page.click('[aria-label="Select and move"]');
 await page.evaluate(()=>{const dt=new DataTransfer();dt.setData('text/plain','https://www.rust-lang.org');document.querySelector('.canvas-scroll').dispatchEvent(new DragEvent('drop',{bubbles:true,dataTransfer:dt,clientX:800,clientY:500}));});
 assert.ok((await read()).notes[0].cards.some(c=>c.text==='https://www.rust-lang.org'));
 await page.evaluate(()=>{
  const raw=atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=');
  const bytes=Uint8Array.from(raw,c=>c.charCodeAt(0));const dt=new DataTransfer();dt.items.add(new File([bytes],'pixel.png',{type:'image/png'}));
  document.querySelector('.canvas-scroll').dispatchEvent(new DragEvent('drop',{bubbles:true,dataTransfer:dt,clientX:700,clientY:420}));
 });
 await page.waitForFunction(()=>document.querySelector('.attachment')?.complete);
 assert.ok((await read()).notes[0].cards.some(c=>c.kind==='image' && c.text.startsWith('data:image/png;base64,')));
 const originalEmail='From: =?UTF-8?B?Sm9zw6k=?= <jose@example.com>\r\nTo: Notes <notes@example.com>\r\nSubject: =?UTF-8?Q?Caf=C3=A9_notes?=\r\nDate: Thu, 29 Oct 2026 10:00:00 +0000\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Bring the sketchbook.</p><script>window.emailExecuted=true</script><img src="https://tracking.invalid/pixel">';
 await page.evaluate(raw=>{
  const dt=new DataTransfer();dt.items.add(new File([raw],'meeting.eml',{type:'message/rfc822'}));
  document.querySelector('.canvas-scroll').dispatchEvent(new DragEvent('drop',{bubbles:true,dataTransfer:dt,clientX:440,clientY:260}));
 },originalEmail);
 await page.waitForSelector('.email-content');
 const emailCard=(await read()).notes[0].cards.find(c=>c.kind==='email');
 assert.equal(emailCard.email.subject,'Café notes');assert.ok(emailCard.email.sender.includes('José'));
 assert.equal(Buffer.from(emailCard.email.original,'base64').toString(),originalEmail);
 assert.ok(await page.$eval('.email-body',el=>el.textContent.includes('Bring the sketchbook.')));
 assert.equal(await page.$eval('.email-content',el=>el.querySelectorAll('script,img,iframe').length),0);
 assert.equal(await page.evaluate(()=>window.emailExecuted),undefined);assert.deepEqual(remoteRequests,[]);
 // Verify that downloading the source preserves the original bytes, not just decoded body text.
 const cdp=await page.createCDPSession();await cdp.send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:data,browserContextId:context.id});
 const emailLink=await page.$('.email-footer a');await emailLink.click();
 let downloaded;for(let i=0;i<50;i++){try{downloaded=await readFile(join(data,'notepad-email.eml'));break;}catch{}await new Promise(r=>setTimeout(r,100));}
 assert.equal(downloaded?.toString(),originalEmail);
 // Invalid imports leave the current workspace unchanged.
 const countBefore=(await read()).notes[0].cards.length;
 const badPath=join(data,'invalid.eml');await writeFile(badPath,'not an email');
 await (await page.$('[aria-label="Add email"]')).uploadFile(badPath);
 await page.waitForFunction(()=>document.querySelector('.status').textContent.includes('email headers'));
 assert.equal((await read()).notes[0].cards.length,countBefore);
 await page.click('.view-tabs button:first-child');
 await page.$eval('.document',el=>{el.value='# Smoke test\n\nMy persistent note.';el.dispatchEvent(new Event('input',{bubbles:true}));});
 await page.click('.topbar .primary');await page.waitForFunction(()=>document.querySelector('.status').textContent.startsWith('Synced with server'));
 const saved=await read();assert.equal(saved.notes[0].body,'# Smoke test\n\nMy persistent note.');assert.ok(saved.revision>0);
 await page.reload({waitUntil:'networkidle0'});await page.waitForSelector('.document');assert.equal(await page.$eval('.document',el=>el.value),saved.notes[0].body);
 await page.click('.view-tabs button:last-child');
 assert.ok(await page.$eval('.email-content',el=>el.textContent.includes('Café notes')));
 assert.deepEqual((await read()).notes[0].cards.find(c=>c.kind==='email'),emailCard);
 await page.screenshot({path:'/tmp/notepad-tested.png'});
 // A file read may finish after the user switches notes. Keep the original destination.
 await page.evaluate(()=>{
  const file=new File(['Subject: Delayed capture\n\nCorrect destination'],'delayed.eml',{type:'message/rfc822'});
  const dt=new DataTransfer();dt.items.add(file);
  // DataTransfer clones File objects, so delay at the prototype until the queued read begins.
  const original=File.prototype.arrayBuffer;
  File.prototype.arrayBuffer=function(){const current=this;return new Promise(resolve=>{window.finishEmailRead=()=>{File.prototype.arrayBuffer=original;original.call(current).then(resolve);};});};
  document.querySelector('.canvas-scroll').dispatchEvent(new DragEvent('drop',{bubbles:true,dataTransfer:dt,clientX:440,clientY:260}));
 });
 await page.click('.new-note');
 await page.evaluate(()=>window.finishEmailRead());
 await page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
 const afterSwitch=await read();assert.ok(afterSwitch.notes[0].cards.some(c=>c.email?.subject==='Delayed capture'));assert.equal(afterSwitch.notes[1].cards.length,0);
 await page.click('.note-row:first-child');await page.click('.view-tabs button:last-child');
 await page.setViewport({width:375,height:812});await page.screenshot({path:'/tmp/notepad-mobile.png'});
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),375);
 await page.click('[aria-label="Toggle notes"]');await page.waitForSelector('.menu-open .sidebar');
 // The browser context is isolated; the server must also use an isolated data directory.
 assert.deepEqual(errors,[]);console.log('PASS: cards, pointer/keyboard movement, undo, ink, dropped link/image/email, email source export and safe rendering, writing, server save, reload recovery, mobile shell; no browser errors');
} finally {await browser.close();await fixture.stop();}
