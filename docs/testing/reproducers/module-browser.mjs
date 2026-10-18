// Actual browser UI to daemon/provider. No routing interception, fake backend or unit runner.
import fs from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
const [url,output]=process.argv.slice(2);
const {chromium}=await import(pathToFileURL(process.env.PERITUS_PLAYWRIGHT||`${process.cwd()}/webui/node_modules/playwright/index.mjs`).href);
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/google-chrome',args:['--no-sandbox','--disable-gpu','--renderer-process-limit=2']});
const report={steps:[],errors:[],actions:[]};
let page;
try {
 page=await browser.newPage({viewport:{width:1440,height:1000}});
 page.on('pageerror',e=>report.errors.push(e.message));
 page.on('request',r=>{if(r.url().endsWith('/api/action')){const p=r.postDataJSON();report.actions.push({command:p.command,text:p.text});}});
 await page.goto(url,{waitUntil:'networkidle'});
 const input=page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});await input.waitFor();
 async function send(text,expected,timeout=60000){
  await input.fill(text);await page.getByRole('button',{name:'Send message',exact:true}).click();
  await page.getByText(expected,{exact:false}).last().waitFor({timeout});
  await fs.writeFile(`${output}/browser-${report.steps.length}.aria.txt`,await page.locator('body').ariaSnapshot());
  report.steps.push(text);
 }
 await send('/modules','Available workflow modules:');
 await send('/module missing cannot execute','Module was not started:');
 await send('/module local.resource Read the one declared notes.txt resource and quote it exactly.','MODULE RESOURCE ORACLE:',180000);
 // Wait for the actual terminal result, not a streamed intermediate claim.
 await page.getByText('Module Resource reader completed.',{exact:false}).last().waitFor({timeout:60000});
 await send('/module status','completed_stages');
 await page.screenshot({path:`${output}/browser-modules.png`,fullPage:true});
 if(report.errors.length)throw new Error(JSON.stringify(report.errors));
 if(report.actions.filter(a=>a.command==='send').length!==4)throw new Error('Expected exactly four human sends: '+JSON.stringify(report.actions));
 console.log('Browser modules list, invalid ID, installed resource/provider workflow and status passed');
} catch(error) {
 if(page){await fs.writeFile(`${output}/browser-failure.aria.txt`,await page.locator('body').ariaSnapshot());await page.screenshot({path:`${output}/browser-failure.png`,fullPage:true});}
 throw error;
} finally {
 await fs.writeFile(`${output}/browser-evidence.json`,JSON.stringify(report,null,2));await fs.writeFile(`${output}/browser-done`,'done');await browser.close();
}
