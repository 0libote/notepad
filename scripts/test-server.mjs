import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
export const password='test-only-notepad-owner-password';
export async function startTestServer(port){
 const data=await mkdtemp(join(tmpdir(),'notepad-test-'));const path=join(data,'password');
 await writeFile(path,password,{mode:0o600});await promisify(execFile)('target/debug/notepad-server',['setup','--password-file',path],{env:{...process.env,NOTEPAD_DATA:data}});await rm(path);
 const url=`http://127.0.0.1:${port}`;
 const server=spawn('target/debug/notepad-server',[],{env:{...process.env,NOTEPAD_DATA:data,NOTEPAD_BIND:`127.0.0.1:${port}`,NOTEPAD_PUBLIC_URL:url},stdio:'ignore'});
 for(let i=0;i<100;i++){try{if((await fetch(url+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
 const login=await fetch(url+'/api/auth/login',{method:'POST',headers:{'content-type':'application/json','x-notepad-request':'1','x-notepad-client':'desktop'},body:JSON.stringify({password})});
 if(!login.ok)throw Error('Test owner login failed');const {token}=await login.json();
 const headers={authorization:`Bearer ${token}`,'x-notepad-request':'1'};
 return {data,url,headers,async stop(){const exited=new Promise(r=>server.once('exit',r));server.kill('SIGINT');if(server.exitCode===null)await exited;await rm(data,{recursive:true,force:true});}};
}
export async function signIn(page){
 await page.waitForSelector('#owner-password');await page.type('#owner-password',password);
 await page.click('.auth-fields .primary');await page.waitForFunction(()=>!document.querySelector('.auth-panel'));
}
