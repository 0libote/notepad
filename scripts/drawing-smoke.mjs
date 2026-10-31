import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {startTestServer,signIn} from './test-server.mjs';
const fixture=await startTestServer(3147),{url,headers}=fixture;
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox','--disable-background-timer-throttling','--disable-renderer-backgrounding']});
const context=await browser.createBrowserContext(),page=await context.newPage();
const errors=[];
page.on('pageerror',e=>errors.push(e.message));
page.on('console',m=>{const text=m.text();if(text.includes('disposed'))errors.push(text);if(m.type()==='warning'&&text.includes('disposed'))errors.push(text);});
const saved=()=>page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');
const disk=async()=>{await saved();return page.evaluate(()=>new Promise((resolve,reject)=>{const open=indexedDB.open('notepad-local',1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{const db=open.result;const get=db.transaction('drafts').objectStore('drafts').get('workspace');get.onsuccess=()=>{db.close();resolve(JSON.parse(get.result));};get.onerror=()=>reject(get.error);};}));};
const close=(a,b,message)=>assert.ok(Math.abs(a-b)<3,message||`${a} ≈ ${b}`);
const server=async()=>{const r=await fetch(url+'/api/workspace',{headers});assert.equal(r.status,200);return r.json();};
async function sync(){await page.bringToFront();await page.waitForFunction(()=>!document.querySelector('.topbar .primary')?.disabled);await page.click('.topbar .primary');await page.waitForFunction(()=>document.querySelector('.status')?.textContent.includes('Synced with server'));}
const activeNoteId=async()=>page.evaluate(()=>{const el=document.querySelector('.note-row.current');return el?.dataset?.noteId||el?.getAttribute('data-note-id')||null;});
const workspaceNote=async()=>{const w=await disk();const id=await activeNoteId();return w.notes.find(n=>n.id===id)||w.notes.at(-1);};
const shapes=async()=>(await workspaceNote()).shapes||[];
const ink=async()=>(await workspaceNote()).ink||[];
const worldTransform=()=>page.$eval('#world .drawing-world,.drawing-world',e=>getComputedStyle(e).transform||e.getAttribute('transform')||'');
async function tool(label){await page.waitForSelector(`.drawing-toolbar [aria-label="${label}"]`);await page.click(`.drawing-toolbar [aria-label="${label}"]`);}
async function dragPoints(x1,y1,x2,y2,steps=12){await page.mouse.move(x1,y1);await page.mouse.down();for(let i=1;i<=steps;i++){await page.mouse.move(x1+(x2-x1)*i/steps,y1+(y2-y1)*i/steps,{steps:1});}await page.mouse.up();await saved();}
async function drawShape(label,x1,y1,x2,y2){await tool(label);await dragPoints(x1,y1,x2,y2);}
const shapeByKind=async kind=>(await shapes()).find(s=>s.kind===kind);
try{
 await page.setViewport({width:1440,height:1000});await page.goto(url,{waitUntil:'networkidle0'});await signIn(page);await saved();
 // Blank note: open rail, create, switch to Canvas (last view tab), close rail if open.
 await page.waitForSelector('[aria-label="Toggle notes"]');await page.click('[aria-label="Toggle notes"]');
 await page.waitForSelector('.new-note');const notesBefore=(await disk()).notes.length;await page.click('.new-note');await saved();
 assert.equal((await disk()).notes.length,notesBefore+1);
 const tabs=await page.$$('.view-tabs button');assert.ok(tabs.length>=2);await tabs.at(-1).click();
 await page.waitForSelector('#drawing-viewport',{timeout:15000});await page.waitForSelector('.drawing-toolbar');
 // Rail closed again so canvas keeps full width (parent may adapt selector).
 try{const railOpen=await page.evaluate(()=>document.querySelector('.app-shell')?.classList.contains('menu-open'));if(railOpen){await page.click('[aria-label="Toggle notes"]');}}catch{}
 // Note Content Write/Read unaffected: Canvas is one view, Write body stays intact.
 const bodyBefore=(await workspaceNote()).body||'';
 await page.click('.view-tabs button:first-child');await page.waitForSelector('.document');assert.equal(await page.$eval('.document',e=>e.value),bodyBefore);
 await (await page.$$('.view-tabs button')).at(-1).click();await page.waitForSelector('#drawing-viewport');
 assert.ok(await page.$('#drawing-viewport'));assert.ok(await page.$('#world .drawing-world,.drawing-world'));
 for(const label of ['Rectangle','Diamond','Ellipse','Arrow','Line','Draw','Text','Eraser','Select and move','Pan'])assert.ok(await page.$(`.drawing-toolbar [aria-label="${label}"]`),`toolbar ${label}`);
 await page.click('[aria-label="Toggle drawing properties"]');assert.equal(await page.$eval('.drawing-properties',e=>getComputedStyle(e).display),'none');
 await page.click('[aria-label="Toggle drawing properties"]');assert.notEqual(await page.$eval('.drawing-properties',e=>getComputedStyle(e).display),'none');
 await page.evaluate(()=>document.fonts.load('24px Excalifont'));assert.equal(await page.evaluate(()=>document.fonts.check('24px Excalifont')),true);
 console.log('PASS: blank note on Canvas, toolbar present, Write body unaffected');
 // Rectangle / diamond / ellipse / arrow / line creation in safe zone x350..800 y180..500.
 await drawShape('Rectangle',350,180,500,280);
 await drawShape('Diamond',550,180,700,280);
 await drawShape('Ellipse',350,320,500,420);
 await drawShape('Arrow',550,320,700,420);
 await drawShape('Line',350,450,500,490);
 await saved();let model=await shapes();assert.equal(model.length,5);
 for(const kind of ['rectangle','diamond','ellipse','arrow','line']){const s=model.find(v=>v.kind===kind);assert.ok(s,kind+' in model');assert.ok(s.id);close(s.x,kind==='diamond'||kind==='arrow'?550:350,kind+' x');assert.ok(s.width>50&&s.height>20,kind+' extent');assert.ok(s.style,'style present');assert.ok(await page.$(`[data-shape-id="${s.id}"]`),`rendered ${kind}`);}
 console.log('PASS: rectangle/diamond/ellipse/arrow/line model content, coordinates, SVG render');
 // Properties on selected rectangle.
 await tool('Select and move');
 const rect0=await shapeByKind('rectangle');
 await page.mouse.click(rect0.x+rect0.width/2+0||400,200);
 // Click center of rectangle in screen space: re-click via rendered node bounding box for robustness.
 const rectBox=await page.$eval(`[data-shape-id="${rect0.id}"]`,e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 await page.mouse.click(rectBox.x,rectBox.y);
 await page.waitForSelector('[aria-label="Resize shape"]');
 await page.click('[aria-label="Stroke #e03131"]');await page.click('[aria-label="Fill #ffec99"]');
 await page.click('[aria-label="Stroke width 4"]');
 await page.$eval('[aria-label="Opacity"]',(el)=>{el.focus();el.value='50';el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));});
 await saved();
 const styled=await shapeByKind('rectangle');
 assert.equal(styled.style.stroke,'#e03131');assert.equal(styled.style.fill,'#ffec99');
 assert.equal(Number(styled.style.stroke_width),4);assert.equal(Number(styled.style.opacity),50);
 console.log('PASS: stroke/fill/width/opacity properties persist to shape style');
 // Resize selected shape via Resize handle.
 const beforeResize={...styled};
 const handle=await page.$eval('[aria-label="Resize shape"]',e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 await page.mouse.move(handle.x,handle.y);await page.mouse.down();await page.mouse.move(handle.x+80,handle.y+60,{steps:8});await page.mouse.up();await saved();
 const resized=await shapeByKind('rectangle');close(resized.width,beforeResize.width+80,'resize width');close(resized.height,beforeResize.height+60,'resize height');
 console.log('PASS: resize handle changes shape geometry');
 // Free text: T tool, click blank, type into .drawing-text-editor, commit with Ctrl+Enter.
 await tool('Text');await page.mouse.click(620,460);
 await page.waitForSelector('.drawing-text-editor');await page.type('.drawing-text-editor','Drawing idea');
 await page.keyboard.down('Control');await page.keyboard.press('Enter');await page.keyboard.up('Control');
 await page.mouse.click(620,120);
 await saved();
 const textShape=(await shapes()).find(s=>s.kind==='text'&&s.text?.includes('Drawing idea'));
 assert.ok(textShape,'text shape with content');assert.ok(await page.$(`[data-shape-id="${textShape.id}"]`));
 console.log('PASS: free text creates text shape with content');
 // Drag moves shape in Select mode.
 await tool('Select and move');
 const dragTarget=(await shapes()).find(s=>s.kind==='ellipse');
 const dragBox=await page.$eval(`[data-shape-id="${dragTarget.id}"]`,e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 await page.mouse.move(dragBox.x,dragBox.y);await page.mouse.down();await page.mouse.move(dragBox.x+60,dragBox.y+40,{steps:8});await page.mouse.up();await saved();
 const dragged=(await shapes()).find(s=>s.id===dragTarget.id);close(dragged.x,dragTarget.x+60,'drag x');close(dragged.y,dragTarget.y+40,'drag y');
 assert.equal((await workspaceNote()).body||'',bodyBefore,'Write body unaffected by drawing');
 console.log('PASS: select-drag moves shape geometry, note body unaffected');
 // Pure panning changes transform but not model.
 const snapshot=JSON.stringify({shapes:await shapes(),ink:await ink()});
 const t0=await worldTransform();
 await tool('Pan');await dragPoints(600,300,500,260);const t1=await worldTransform();
 assert.equal(JSON.stringify({shapes:await shapes(),ink:await ink()}),snapshot,'pan keeps model');
 assert.notEqual(t0,t1,'pan changes world transform');
 // Space-drag also pans without model change.
 await tool('Select and move');await page.keyboard.down('Space');await dragPoints(600,300,650,330);await page.keyboard.up('Space');
 assert.equal(JSON.stringify({shapes:await shapes(),ink:await ink()}),snapshot,'space-pan keeps model');
 console.log('PASS: pan/hand/space changes view transform only');
 // Multi-select via shift-click then Delete, Undo/Redo via aria labels.
 await tool('Select and move');
 const r1=await page.$eval(`[data-shape-id="${(await shapeByKind('rectangle')).id}"]`,e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 const d1=await page.$eval(`[data-shape-id="${(await shapeByKind('diamond')).id}"]`,e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 await page.mouse.click(r1.x,r1.y);await page.keyboard.down('Shift');await page.mouse.click(d1.x,d1.y);await page.keyboard.up('Shift');
 const countBefore=(await shapes()).length;
 await page.keyboard.press('Delete');await saved();
 assert.ok((await shapes()).length===countBefore-2,'shift-selection deletes both selected shapes');
 await page.click('[aria-label="Undo"]');await saved();assert.equal((await shapes()).length,countBefore,'undo restores');
 await page.click('[aria-label="Redo"]');await saved();assert.ok((await shapes()).length<countBefore,'redo re-applies');
 await page.click('[aria-label="Undo"]');await saved();assert.equal((await shapes()).length,countBefore,'undo again restores');
 console.log('PASS: shift multi-select, Delete, Undo/Redo restore model');
 // Styled ink then eraser.
 const inkBefore=(await ink()).length;
 await tool('Draw');
 await page.mouse.move(650,350);await page.mouse.down();for(let i=1;i<=10;i++){await page.mouse.move(650+i*10,350+i*10,{steps:1});}await page.mouse.up();await saved();
 assert.equal((await ink()).length,inkBefore+1);
 const stroke=await page.evaluate(()=>{const all=[...document.querySelectorAll('[data-ink-id]')];return all.length;});
 assert.ok(stroke>=1,'ink rendered');
 await tool('Eraser');
 const inkId=(await ink()).at(-1).id;
 const inkBox=await page.$eval(`[data-ink-id="${inkId}"]`,e=>{const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};});
 await page.mouse.move(inkBox.x,inkBox.y);await page.mouse.down();await page.mouse.move(inkBox.x+10,inkBox.y+10,{steps:4});await page.mouse.up();await saved();
 assert.equal((await ink()).length,inkBefore,'eraser removes ink');
 await page.click('[aria-label="Undo"]');await saved();assert.equal((await ink()).length,inkBefore+1,'undo restores ink');
 console.log('PASS: freehand ink creates points, eraser removes, undo restores');
 // Ctrl-wheel zoom changes scale; coordinate mapping still sane for next shape.
 await page.mouse.move(850,500);const z0=await worldTransform();
 await page.keyboard.down('Control');await page.mouse.wheel({deltaY:-240});await page.keyboard.up('Control');await saved();
 await page.evaluate(()=>new Promise(requestAnimationFrame));const z1=await worldTransform();
 assert.notEqual(z0,z1,'ctrl-wheel must zoom');{
  const n0=(await shapes()).length;await drawShape('Rectangle',360,200,460,260);await saved();
  assert.equal((await shapes()).length,n0+1);const fresh=(await shapes()).at(-1);assert.ok(Number.isFinite(fresh.x)&&Number.isFinite(fresh.y),'zoom keeps coordinate mapping finite');
  await page.click('[aria-label="Undo"]');await saved();
  console.log('PASS: ctrl-wheel zoom rescales view, drawing still maps to world');
 }
 // Persistence: IndexedDB draft matches server Save and reload.
 await sync();await saved();
 const local=await disk();const remote=await server();
 const noteId=(await workspaceNote()).id;const localNote=local.notes.find(n=>n.id===noteId);
 const remoteNote=remote.notes.find(n=>n.id===noteId);
 assert.deepEqual(remoteNote.shapes||[],localNote.shapes||[]);assert.deepEqual(remoteNote.ink||[],localNote.ink||[]);
 await page.reload({waitUntil:'networkidle0'});await saved();
 const afterReload=await workspaceNote();
 assert.deepEqual(afterReload.shapes||[],localNote.shapes||[]);assert.deepEqual(afterReload.ink||[],localNote.ink||[]);
 console.log('PASS: vector data persists through server Save and reload');
 // Mobile: toolbar contained, no document overflow.
 await page.setViewport({width:375,height:812});
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth+1),true);
 assert.equal(await page.evaluate(()=>{const t=document.querySelector('.drawing-toolbar');const r=t.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth;}),true);
 assert.deepEqual(errors,[]);
 console.log('PASS: mobile 375px toolbar containment, no browser errors');
}catch(error){await page.screenshot({path:'/tmp/notepad-drawing-failure.png'}).catch(()=>{});try{console.error(await page.$eval('.status',e=>e.textContent));}catch{}console.error(errors);throw error;}finally{await browser.close();await fixture.stop();}
