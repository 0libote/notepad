import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {startTestServer,signIn} from './test-server.mjs';
const fixture=await startTestServer(3142),{url,data,headers}=fixture;
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox','--disable-background-timer-throttling','--disable-renderer-backgrounding']});
const errors=[];const context=await browser.createBrowserContext(),page=await context.newPage();
page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.text().includes('disposed'))errors.push(m.text());});
const saved=()=>page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
const disk=async()=>{await saved();return page.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result,r=db.transaction('drafts').objectStore('drafts').get('workspace');r.onsuccess=()=>{db.close();resolve(JSON.parse(r.result));};};}));};
const note=async()=>(await disk()).notes[0];
const card=async id=>(await note()).cards.find(c=>c.id===id);
const close=(a,b,message)=>assert.ok(Math.abs(a-b)<2,message||`${a} ≈ ${b}`);
const focus=selector=>page.$eval(selector,e=>e.focus());
const undo=async()=>{await focus('#world');await page.keyboard.down('Control');await page.keyboard.press('z');await page.keyboard.up('Control');await saved();};
const rect=selector=>page.$eval(selector,e=>{const r=e.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};});
async function drag(selector,dx,dy){const r=await rect(selector);await page.mouse.move(r.x+r.width/2,r.y+r.height/2);await page.mouse.down();await page.mouse.move(r.x+r.width/2+dx,r.y+r.height/2+dy,{steps:8});await page.mouse.up();await saved();}
const server=async()=>{const r=await fetch(url+'/api/workspace',{headers});assert.equal(r.status,200);return r.json();};
async function sync(p=page){await p.bringToFront();await p.waitForFunction(()=>!document.querySelector('.topbar .primary')?.disabled);await p.click('.topbar .primary');await p.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));}
try{
 await page.setViewport({width:1440,height:1000});await page.goto(url+'/#canvas',{waitUntil:'networkidle0'});await signIn(page);await saved();
 assert.equal((await note()).cards.length,3);assert.equal((await card('idea')).size,undefined);
 await drag('#canvas-card-idea .card-handle',40,20);close((await card('idea')).x,160);close((await card('idea')).y,120);
 const before=await rect('#canvas-card-idea');await drag('#canvas-card-idea .resize-handle',140,90);
 let resized=await card('idea');close(resized.size.width,before.width+140);close(resized.size.height,before.height+90);close(resized.x,160);
 await undo();assert.equal((await card('idea')).size,undefined);close((await card('idea')).x,160);
 await focus('#canvas-card-idea .card-handle');await focus('#canvas-card-idea .resize-handle');await page.keyboard.press('ArrowRight');await page.keyboard.down('Shift');await page.keyboard.press('ArrowDown');await page.keyboard.up('Shift');await saved();
 resized=await card('idea');close(resized.size.width,before.width+10);close(resized.size.height,before.height+40);close(resized.y,120);
 await undo();await undo();assert.equal((await card('idea')).size,undefined);
 console.log('PASS: legacy layout, pointer resize, keyboard resize, and gesture Undo');
 await page.click('[aria-label="Connect cards"]');await page.click('#canvas-card-idea .card-handle');await page.click('#canvas-card-sticky .card-handle');await saved();
 assert.equal((await note()).connections.length,1);const edge=(await note()).connections[0];assert.equal(edge.from,'idea');assert.equal(edge.to,'sticky');
 let path=await page.$eval('.connection-line',e=>e.getAttribute('d'));await page.evaluate(()=>{window.originalCard=document.getElementById('canvas-card-sticky');window.originalEdge=document.querySelector('.connection');});
 await drag('#canvas-card-sticky .card-handle',180,80);let moved=await page.$eval('.connection-line',e=>e.getAttribute('d'));assert.notEqual(moved,path);
 assert.equal(await page.evaluate(()=>window.originalCard===document.getElementById('canvas-card-sticky')&&window.originalEdge===document.querySelector('.connection')),true,'Moving preserves the card and connector DOM nodes');
 await focus('.connection');await page.keyboard.press('Delete');await saved();assert.equal((await note()).connections?.length||0,0);await undo();assert.equal((await note()).connections[0].id,edge.id);
 await focus('#canvas-card-idea .card-handle');await drag('#canvas-card-idea .resize-handle',80,60);assert.notEqual(await page.$eval('.connection-line',e=>e.getAttribute('d')),moved);
 // Backspace in text edits text instead of deleting the containing card or its connections.
 await focus('#canvas-card-idea textarea');await page.$eval('#canvas-card-idea textarea',e=>e.setSelectionRange(e.value.length,e.value.length));await page.keyboard.press('Backspace');await saved();assert.equal((await note()).cards.length,3);assert.equal((await note()).connections.length,1);
 await focus('#canvas-card-idea .card-handle');await page.keyboard.down('Control');await page.keyboard.press('d');await page.keyboard.up('Control');await saved();
 let n=await note();assert.equal(n.cards.length,4);const copy=n.cards.find(c=>!['idea','sticky','reference'].includes(c.id));assert.deepEqual(copy.size,n.cards[0].size);assert.equal(n.connections.length,1);
 await page.keyboard.press('Delete');await saved();assert.equal((await note()).cards.length,3);await undo();assert.equal((await note()).cards.length,4);await undo();assert.equal((await note()).cards.length,3);
 await focus('#canvas-card-idea .card-handle');await page.keyboard.press('Backspace');await saved();assert.equal((await note()).cards.length,2);assert.equal((await note()).connections?.length||0,0);await undo();assert.equal((await note()).cards.length,3);assert.equal((await note()).connections[0].id,edge.id);
 console.log('PASS: anchored connectors, stable DOM, selection deletion, cascading removal, duplication, and text-editing safeguards');
 // Freehand paths and individual dots can be selected and removed independently.
 await page.click('[aria-label="Draw"]');let world=await rect('#world');await page.mouse.move(world.x+900,world.y+450);await page.mouse.down();await page.mouse.move(world.x+1000,world.y+490,{steps:8});await page.mouse.up();await page.mouse.click(world.x+940,world.y+540);await saved();assert.equal((await note()).strokes.length,2);
 await page.click('[aria-label="Select and move"]');await page.mouse.click(world.x+950,world.y+470);await page.keyboard.press('Delete');await saved();assert.equal((await note()).strokes.length,1);await undo();assert.equal((await note()).strokes.length,2);
 await page.click('[aria-label="Draw"]');const strokeCount=(await note()).strokes.length;
 await page.$eval('#world',el=>{const r=el.getBoundingClientRect();const send=(type,id,x,y)=>el.dispatchEvent(new PointerEvent(type,{pointerId:id,pointerType:'touch',isPrimary:id===41,clientX:r.x+x,clientY:r.y+y,bubbles:true}));send('pointerdown',41,880,560);send('pointerdown',42,890,570);send('pointercancel',42,890,570);send('pointermove',41,900,580);send('pointerup',41,900,580);});
 await saved();assert.equal((await note()).strokes.length,strokeCount+1);assert.equal((await note()).strokes.at(-1).length,2);await undo();assert.equal((await note()).strokes.length,strokeCount);
 await page.click('[aria-label="Connect cards"]');await page.click('#canvas-card-idea .card-handle');await page.keyboard.press('Escape');assert.equal(await page.$eval('[aria-label="Connect cards"]',e=>e.classList.contains('chosen')),false);
 // Coordinate conversion respects zoom, scrolling, and focus without jumping the viewport.
 await page.click('[aria-label="Zoom out"]');await page.click('[aria-label="Zoom out"]');await page.$eval('.canvas-scroll',e=>{e.scrollLeft=300;e.scrollTop=150;});
 const scroll=await page.$eval('.canvas-scroll',e=>({x:e.scrollLeft,y:e.scrollTop}));const beforeMove=await card('sticky');await drag('#canvas-card-sticky .card-handle',80,40);const afterMove=await card('sticky');close(afterMove.x,beforeMove.x+100);close(afterMove.y,beforeMove.y+50);
 assert.deepEqual(await page.$eval('.canvas-scroll',e=>({x:e.scrollLeft,y:e.scrollTop})),scroll);
 const small=await rect('#canvas-card-sticky');await drag('#canvas-card-sticky .resize-handle',80,40);const newSize=(await card('sticky')).size;close(newSize.width,small.width/0.8+100);close(newSize.height,small.height/0.8+50);
 console.log('PASS: selectable ink and dots, gesture ownership, cancellation, zoom, scrolling, and resize coordinates');
 await sync();await saved();let w=await disk();let remote=await server();assert.deepEqual(remote.notes,w.notes);
 await page.reload({waitUntil:'networkidle0'});await saved();assert.deepEqual((await disk()).notes,w.notes);assert.equal(await page.$eval('.connection-line',e=>!!e.getAttribute('d')),true);
 // An old client at the correct revision must not silently drop size/connection fields.
 const oldClient=structuredClone(remote);oldClient.schema=1;for(const n of oldClient.notes){delete n.connections;for(const c of n.cards)delete c.size;}
 const rejected=await fetch(url+'/api/workspace',{method:'PUT',headers:{...headers,'content-type':'application/json'},body:JSON.stringify(oldClient)});assert.equal(rejected.status,422);assert.deepEqual(await server(),remote);
 // A second profile loads and edits the same geometry through the real authenticated API.
 const secondContext=await browser.createBrowserContext(),second=await secondContext.newPage();second.on('pageerror',e=>errors.push(e.message));await second.goto(url+'/#canvas',{waitUntil:'networkidle0'});await signIn(second);await second.waitForSelector('#canvas-card-idea');
 await second.$eval('#canvas-card-idea .card-handle',e=>e.focus());await second.$eval('#canvas-card-idea .resize-handle',e=>e.focus());await second.keyboard.press('ArrowRight');await sync(second);remote=await server();
 await sync();await saved();assert.deepEqual((await disk()).notes,remote.notes);assert.equal((await note()).connections[0].id,edge.id);await secondContext.close();
 // Every capture surface participates in the same resize model without losing its content.
 await page.click('.new-note');await page.$eval('[aria-label="Note title"]',e=>{e.value='Capture sizing';e.dispatchEvent(new Event('input',{bubbles:true}));});await page.click('.view-tabs button:last-child');
 const imagePath=join(data,'pixel.png');await writeFile(imagePath,Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aRWQAAAAASUVORK5CYII=','base64'));
 await (await page.$('[aria-label="Add image"]')).uploadFile(imagePath);
 const mailPath=join(data,'resize.eml');await writeFile(mailPath,'From: Writer <writer@example.test>\r\nTo: Reader <reader@example.test>\r\nSubject: Keep this message\r\n\r\nThe complete email remains available after resizing.');
 await (await page.$('[aria-label="Add email"]')).uploadFile(mailPath);
 await page.click('[aria-label="Add note to canvas"]');await page.select('#linked-note','welcome');await page.click('.note-picker .primary');
 await page.click('[aria-label="Add website"]');await page.type('#website-url','https://example.test/reference');await page.click('.link-form .primary');
 await page.waitForFunction(()=>document.querySelectorAll('.canvas-card').length===4);await saved();
 let captures=(await disk()).notes.find(n=>n.title==='Capture sizing');
 for(const c of captures.cards){const selector='[data-card-id="'+c.id+'"]';await focus(selector+' .card-handle');await focus(selector+' .resize-handle');await page.keyboard.press('ArrowRight');await saved();const current=(await disk()).notes.find(n=>n.id===captures.id).cards.find(item=>item.id===c.id);const r=await rect(selector);close(r.width,current.size.width);close(r.height,current.size.height);assert.equal(current.text,c.text);assert.deepEqual(current.email,c.email);}
 assert.equal(await page.$eval('.attachment',e=>getComputedStyle(e).objectFit),'contain');assert.equal(await page.$eval('.email-footer a',e=>e.download),'notepad-email.eml');assert.equal(await page.$eval('.open-note',e=>e.textContent),'Open note');
 await sync();await saved();assert.deepEqual((await server()).notes,(await disk()).notes);await page.click('.note-row:first-child');await page.waitForSelector('#canvas-card-idea');
 console.log('PASS: image, email, website, and linked-page resizing preserves content and persists through the API');
 // An old backup upgrades at import without changing its page content.
 const legacy=structuredClone(await disk());legacy.schema=1;for(const n of legacy.notes){delete n.connections;for(const c of n.cards)delete c.size;}
 const legacyPath=join(data,'legacy.json');await writeFile(legacyPath,JSON.stringify(legacy));
 // Keep a current backup so this import test can restore the full board afterward.
 const currentPath=join(data,'current.json');await writeFile(currentPath,JSON.stringify(await disk()));page.on('dialog',d=>d.accept());
 await (await page.$('[aria-label="Import backup"]')).uploadFile(legacyPath);await page.waitForFunction(()=>!document.querySelector('.connection'));await saved();assert.equal((await disk()).schema,remote.schema);assert.equal((await note()).body,legacy.notes[0].body);
 await page.waitForFunction(()=>!document.querySelector('.topbar .primary')?.disabled);await (await page.$('[aria-label="Import backup"]')).uploadFile(currentPath);await page.waitForSelector('.connection');await saved();
 // Dangling connections cannot replace the current notebook.
 const invalid=structuredClone(await disk());invalid.notes[0].connections[0].to='missing';const invalidPath=join(data,'invalid-canvas.json');await writeFile(invalidPath,JSON.stringify(invalid));await (await page.$('[aria-label="Import backup"]')).uploadFile(invalidPath);await page.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Invalid backup'));assert.equal((await note()).connections[0].to,'sticky');
 await focus('#canvas-card-idea .card-handle');await page.screenshot({path:'/tmp/notepad-canvas-editing.png'});
 await page.setViewport({width:375,height:812});await page.$eval('.canvas-scroll',e=>{e.scrollLeft=0;e.scrollTop=0;});await page.screenshot({path:'/tmp/notepad-canvas-editing-mobile.png'});
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);assert.equal(await page.$eval('.toolbar',e=>e.scrollWidth<=e.clientWidth),true);
 assert.deepEqual(errors,[]);console.log('PASS: authenticated persistence, reload, two-profile geometry sync, invalid import protection, mobile containment; no browser errors');
}catch(error){await page.screenshot({path:'/tmp/notepad-canvas-failure.png'}).catch(()=>{});console.error(await page.$eval('.status',e=>e.textContent).catch(()=>''));console.error(errors);throw error;}finally{await browser.close();await fixture.stop();}
