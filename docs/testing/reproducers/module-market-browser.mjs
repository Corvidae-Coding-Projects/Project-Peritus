// Real GUI controls and persisted jobs from the actual TUI campaign. No substituted backend.
import fs from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
const [url,output]=process.argv.slice(2);
const {chromium}=await import(pathToFileURL(process.env.PERITUS_PLAYWRIGHT||`${process.cwd()}/webui/node_modules/playwright/index.mjs`).href);
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/google-chrome',args:['--no-sandbox','--disable-gpu','--renderer-process-limit=2']});
const report={steps:[],errors:[],actions:[]};let page;
try{
 const runFiles=await fs.readdir(`${output}/instance/state/peritus/daemon/workbench-v1/runs`);const runId=runFiles.find(n=>n.endsWith('.json')).slice(0,-5);
 const registry=JSON.parse(await fs.readFile(`${output}/instance/data/peritus/modules/settings.json`,'utf8'));const digest=registry.modules['peritus.deep-research'].digest;
 const runRecord=`${output}/instance/state/peritus/daemon/workbench-v1/runs/${runId}.json`;
 const state=`${output}/instance/state/peritus/daemon/product-runs/${runId}.modules/native-peritus.deep-research-${digest}/state`;
 page=await browser.newPage({viewport:{width:1440,height:1050}});page.setDefaultTimeout(45000);page.on('pageerror',e=>report.errors.push(e.message));
 page.on('request',r=>{if(r.url().endsWith('/api/action')){const v=r.postDataJSON();report.actions.push({command:v.command,action:v.action,module:v.module,text:v.text});}});
 await page.goto(url,{waitUntil:'networkidle'});
 const input=page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});await input.waitFor();
 const item=()=>page.getByRole('article',{name:'Deep Research',exact:true});
 async function menu(){await page.getByRole('button',{name:'Modules',exact:true}).click();await page.getByRole('heading',{name:'Deep Research',exact:false}).waitFor();}
 async function step(value){report.steps.push(value);await fs.writeFile(`${output}/browser-progress.json`,JSON.stringify(report,null,2));}
 async function capture(name){await fs.writeFile(`${output}/${name}.aria.txt`,await page.locator('body').ariaSnapshot());await page.screenshot({path:`${output}/${name}.png`,fullPage:true});}
 async function idle(){await page.locator('.working-observation').waitFor({state:'hidden',timeout:60000});}
 async function observedIdle(count){
  await page.waitForFunction(count=>{const term=[...document.querySelectorAll('.signal-readout span')].find(e=>e.textContent.trim()==='Input received');return term?.nextElementSibling?.textContent.trim()===String(count)&&!document.querySelector('.working-observation');},count,{timeout:60000});
 }
 async function human(button,command){
  const before=JSON.parse(await fs.readFile(runRecord,'utf8')).interaction.public_input_count;
  await button.click();let count;
  await waitFor(async()=>{const r=JSON.parse(await fs.readFile(runRecord,'utf8'));const users=r.interaction.activities.filter(a=>a[1]===1);count=r.interaction.public_input_count;return count>before&&users.at(-1)?.[2]===command&&r.phase===11;},240);
  await observedIdle(count);
 }

 async function waitFor(check,seconds=90){const end=Date.now()+seconds*1000;while(Date.now()<end){if(await check())return;await page.waitForTimeout(150);}throw new Error('Timed out waiting for independent program readback');}
 // Open the retained real checked report through the actual Runs UI.
 await page.getByRole('button',{name:'Runs',exact:true}).click();await page.locator('.library-row').filter({hasText:runId.slice(0,8)}).getByRole('button',{name:'Open conversation',exact:true}).click();await input.waitFor();await idle();
 await menu();let row=item();const search=page.getByRole('textbox',{name:'Search modules'});await search.fill('does-not-match-a-plugin');await page.getByText('No modules match this search.').waitFor();await search.fill('Deep Research');await step('Filter empty and matching catalog rows');
 await page.getByRole('button',{name:'Refresh market',exact:true}).click();await page.getByRole('button',{name:'Refresh market',exact:true}).waitFor();
 await row.getByRole('button',{name:'Disable',exact:true}).click();await row.getByRole('button',{name:'Remove',exact:true}).click();
 await row.getByRole('button',{name:'Install (disabled)',exact:true}).click();await row.getByRole('button',{name:'Enable',exact:true}).waitFor({timeout:90000});await row.getByRole('button',{name:'Enable',exact:true}).click();
 await row.getByRole('button',{name:'Use module',exact:true}).click();if(await input.inputValue()!=='/module peritus.deep-research ')throw new Error('Native module selection did not prepare generic command');await step('Refresh, disable, remove, install disabled, enable and select executable Rust package');
 await menu();row=item();await human(row.getByRole('button',{name:'View report',exact:true}),'/module peritus.deep-research show');await page.getByText('Source-checked report',{exact:false}).last().waitFor({timeout:60000});await idle();await capture('browser-native-checked-report');
 await menu();row=item();await human(row.getByRole('button',{name:'Use in Design',exact:true}),'/module peritus.deep-research use-in-design');await page.getByText("This exact source-checked report is ready",{exact:false}).last().waitFor();await idle();await step('Plugin-supplied checked report and explicit Design artifact controls on actual completed research');
 const topic='According only to the official Rust Book at https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html, what happens when an owner goes out of scope? One short plain-language answer, current documentation scope, no code changes and no other preferences needed.';
 const oldEpisode=JSON.parse(await fs.readFile(`${state}/selected.json`,'utf8')).episode;
 await input.fill('/module peritus.deep-research '+topic);await human(page.getByRole('button',{name:'Send message',exact:true}),'/module peritus.deep-research '+topic);
 await waitFor(async()=>{const p=JSON.parse(await fs.readFile(`${state}/selected.json`,'utf8'));const episode=JSON.parse(await fs.readFile(`${state}/jobs/${p.episode}/episode.json`,'utf8'));return p.episode!==oldEpisode&&episode.phase===1;},240);await idle();await capture('browser-native-plan');
 const current=JSON.parse(await fs.readFile(`${state}/selected.json`,'utf8')).episode;const episodeFile=`${state}/jobs/${current}/episode.json`;const reads=`${state}/jobs/${current}/research/reads/gather`;
 try{await fs.stat(reads);throw new Error('Gathering occurred before Start');}catch(e){if(e.code!=='ENOENT')throw e;}
 await menu();row=item();await row.getByRole('button',{name:'Start research',exact:true}).waitFor();await row.getByRole('button',{name:'Start research',exact:true}).click();
 await waitFor(async()=>JSON.parse(await fs.readFile(episodeFile,'utf8')).phase>=2,300);await page.getByRole('button',{name:'Stop work',exact:false}).click();await idle();
 const stopped=JSON.parse(await fs.readFile(episodeFile,'utf8'));if(stopped.phase<2||stopped.phase>=5)throw new Error('Expected explicit stop with committed gathering and retained unfinished work');
 await step('Fresh real-provider plan, no pre-Start gathering, generic Start research, actual source collection and Stop control');await capture('browser-native-start-stop');
 if(report.errors.length)throw new Error(JSON.stringify(report.errors));await fs.writeFile(`${output}/browser-success`,'PASS');
}catch(error){if(page){await fs.writeFile(`${output}/browser-failure.aria.txt`,await page.locator('body').ariaSnapshot());await page.screenshot({path:`${output}/browser-failure.png`,fullPage:true});}throw error;}
finally{await fs.writeFile(`${output}/browser-evidence.json`,JSON.stringify(report,null,2));await fs.writeFile(`${output}/browser-done`,'done');await browser.close();}
