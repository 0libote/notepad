import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {startTestServer} from './test-server.mjs';
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox']});
try{
 for(const [i,password] of ['x',''].entries()){
  const fixture=await startTestServer(3143+i,{configured:false});const context=await browser.createBrowserContext(),page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
   await page.goto(fixture.url,{waitUntil:'networkidle0'});await page.waitForSelector('#owner-password');
   assert.equal(await page.$eval('.auth-panel h2',e=>e.textContent),'Create your notebook');assert.equal(await page.$eval('#owner-password',e=>e.required),false);assert.equal(await page.$eval('#owner-password',e=>e.getAttribute('minlength')),null);assert.equal(await page.$eval('#owner-password',e=>e.getAttribute('maxlength')),null);
   await page.type('#owner-password',password);await page.click('.auth-fields .primary');await page.waitForFunction(()=>!document.querySelector('.auth-panel'));await page.waitForSelector('.canvas-card');
   assert.equal((await page.evaluate(async()=> (await fetch('/api/workspace')).status)),200);
   const reset=await page.evaluate(async()=> (await fetch('/api/auth/setup',{method:'POST',headers:{'content-type':'application/json','x-notepad-request':'1'},body:JSON.stringify({password:'replacement'})})).status);assert.equal(reset,409);
   await page.waitForFunction(()=>document.querySelector('.statusbar')?.dataset.localState==='saved');await page.reload({waitUntil:'networkidle0'});await page.waitForSelector('.canvas-card');
   page.on('dialog',d=>d.accept());await page.click('.session-control:not(.hidden-control)');await page.waitForSelector('#owner-password');assert.equal(await page.$eval('.auth-panel h2',e=>e.textContent),'Sign in to your notebook');
   await page.type('#owner-password',password);await page.click('.auth-fields .primary');await page.waitForSelector('.canvas-card');
   const other=await browser.createBrowserContext(),second=await other.newPage();await second.goto(fixture.url,{waitUntil:'networkidle0'});await second.waitForSelector('#owner-password');assert.equal(await second.$eval('.auth-panel h2',e=>e.textContent),'Sign in to your notebook');await other.close();
   await page.setViewport({width:375,height:812});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);assert.deepEqual(errors,[]);
   console.log('PASS: first-load '+(password?'one-character':'empty')+' password, immediate sign-in, reload, sign-out/sign-in, later-user protection and mobile');
  }finally{await context.close();await fixture.stop();}
 }
}finally{await browser.close();}
