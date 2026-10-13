#!/usr/bin/env node
// Real application replay for GitHub #122. No request interception, fake pages,
// model submissions, unit tests, or repository verification commands are used.
// Oracle comes from the requested capabilities, not the application's strings.
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const baseURL = process.env.PERITUS_REPLAY_URL || 'http://127.0.0.1:4221';
const out = process.env.PERITUS_REPLAY_OUTPUT || '/tmp/peritus-ui-issues-20261013/composer-evidence';
const modulePath = process.env.PERITUS_PLAYWRIGHT || '/home/doll/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs';
const { chromium } = await import(pathToFileURL(modulePath).href);
const seed = 12220261013;
let rng = seed >>> 0;
const random = () => { rng ^= rng << 13; rng ^= rng >>> 17; rng ^= rng << 5; return (rng >>> 0) / 2**32; };
const modes = ['chat', 'plan', 'review', 'build'];
const viewports = [{width:1440,height:1000}, {width:768,height:1024}, {width:360,height:800}];
const drafts = ['', 'Please inspect the build script; do not change mode automatically.', '明日の計画 🧭\nReview first; changes need explicit permission.', 'café / résumé — e\u0301\nأهلاً\n🛠️ Keep this draft.', 'line one\nline two\nline three', 'x'.repeat(2048) + '\n終'];
const report = { version:1, endpoint:baseURL, seed, started:new Date().toISOString(), scenarios:[], traces:[], snapshots:[], errors:[], console:[], responses:[], mutations:[], boundedBudget:{viewports:viewports.length, casesPerViewport:6, switchesPerCase:4, generatedCases:0, generatedSwitches:0} };
const check = (name, pass, actual, expected) => { report.scenarios.push({name,status:pass?'PASS':'FAIL',expected,actual}); return pass; };
const semantic = (mode, text) => {
  const common = typeof text==='string' && text.length>40;
  if(mode==='chat')return common && /discuss|conversation|inspect/i.test(text) && /explicitly authorized/i.test(text) && /build pipeline/i.test(text) && /permissions/i.test(text);
  if(mode==='build')return common && /design/i.test(text) && /writer/i.test(text) && /checks/i.test(text) && /reviewer/i.test(text) && /fixer/i.test(text) && /pipeline/i.test(text) && /permissions/i.test(text);
  return common && /read.only/i.test(text) && /cannot edit files or run commands/i.test(text) && /switch explicitly/i.test(text) && /\/chat/i.test(text) && /\/build/i.test(text) && (mode==='plan'?/planning/i.test(text):/findings/i.test(text));
};
await fs.mkdir(out,{recursive:true});
const browser = await chromium.launch({headless:true,executablePath:process.env.PERITUS_CHROME || '/usr/bin/google-chrome',args:['--no-sandbox']});
let activeContext;
try {
  for(const viewport of viewports){
    activeContext = await browser.newContext({viewport});
    const page = await activeContext.newPage();
    const label = `${viewport.width}x${viewport.height}`;
    page.on('pageerror',error=>report.errors.push({viewport:label,type:'pageerror',message:error.message}));
    page.on('console',message=>{if(['error','warning'].includes(message.type()))report.console.push({viewport:label,type:message.type(),text:message.text()});});
    page.on('response',response=>{if(response.status()>=400)report.responses.push({viewport:label,status:response.status(),url:response.url()});});
    page.on('request',request=>{
      if(!['GET','HEAD'].includes(request.method())){
        let command=null;try{command=JSON.parse(request.postData()||'{}').command??null;}catch{}
        report.mutations.push({viewport:label,method:request.method(),url:request.url(),command});
      }
    });
    await page.goto(baseURL,{waitUntil:'networkidle'});
    const input = page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});
    if(!(await input.count())){
      const newConversation = page.getByRole('button',{name:'New conversation',exact:true});
      if(await newConversation.count())await newConversation.click();
    }
    await input.waitFor({state:'visible',timeout:15000});
    const button = mode => page.locator('.mode-switch').getByRole('button',{name:mode,exact:true});
    const current = async() => (await page.locator('.mode-switch button[aria-pressed="true"]').textContent()).trim();
    const snapshot = async(name) => {
      const filename = `${label}-${name}`;
      const data = await page.locator('.composer').evaluate(element=>({text:element.innerText,html:element.outerHTML,viewport:{width:innerWidth,height:innerHeight}}));
      await fs.writeFile(path.join(out,`${filename}.json`),JSON.stringify(data,null,2));
      await fs.writeFile(path.join(out,`${filename}.aria.txt`),await page.locator('.composer').ariaSnapshot());
      await page.screenshot({path:path.join(out,`${filename}.png`),fullPage:true});
      report.snapshots.push(filename);
    };
    const hint = async(mode,phase) => {
      const observation = await page.locator('#composer-mode-hint').evaluate(element=>{
        const box=element.getBoundingClientRect();const style=getComputedStyle(element);
        const range=document.createRange();range.selectNodeContents(element);
        const textBoxes=[...range.getClientRects()].map(rect=>({left:rect.left,right:rect.right,top:rect.top,bottom:rect.bottom}));
        return {text:element.textContent,box:{x:box.x,y:box.y,width:box.width,height:box.height,right:box.right,bottom:box.bottom},style:{display:style.display,visibility:style.visibility,overflow:style.overflow},textBoxes,viewport:{width:innerWidth,height:innerHeight},document:{width:document.documentElement.scrollWidth,height:document.documentElement.scrollHeight}};
      });
      check(`${label} ${phase} ${mode} visible hint`,observation.box.width>0 && observation.box.height>0 && observation.box.x>=0 && observation.box.right<=viewport.width+1 && observation.box.y>=0 && observation.box.bottom<=viewport.height+1 && observation.style.visibility==='visible' && observation.textBoxes.every(box=>box.left>=0 && box.right<=viewport.width+1 && box.top>=0 && box.bottom<=viewport.height+1),observation,'Persistent untruncated composer capability hint fully visible in viewport');
      check(`${label} ${phase} ${mode} hint semantics`,semantic(mode,observation.text) && /mode changes are explicit/i.test(observation.text),observation.text,'Requested capability explanation plus explicit mode changes');
      return observation;
    };
    const cdp = await activeContext.newCDPSession(page);
    await cdp.send('Accessibility.enable');
    for(const mode of modes){
      await button(mode).click();
      await button(mode).focus();
      check(`${label} ${mode} keyboard focus`,await button(mode).evaluate(element=>element===document.activeElement),await button(mode).evaluate(element=>document.activeElement?.outerHTML),'Mode button can receive keyboard focus');
      const dom = await button(mode).evaluate(element=>({title:element.title,description:[...(element.getAttribute('aria-describedby')||'').split(/\s+/)].filter(Boolean).map(id=>document.getElementById(id)?.textContent||'').join(' '),pressed:element.getAttribute('aria-pressed')}));
      const tree = await cdp.send('Accessibility.getFullAXTree');
      const ax = tree.nodes.filter(node=>node.role?.value==='button' && node.name?.value?.toLowerCase()===mode).map(node=>({name:node.name?.value,description:node.description?.value,properties:node.properties}));
      check(`${label} ${mode} title capabilities`,semantic(mode,dom.title),dom.title,'Title describes the requested mode capabilities');
      check(`${label} ${mode} accessible capabilities`,semantic(mode,dom.description) && ax.some(node=>node.description===dom.title),{dom,ax},'Accessible description exposes the same capabilities as title');
      await hint(mode,'direct');
      await snapshot(mode);
      const before = await current();
      await page.locator('.composer-foot button').click();
      await page.getByRole('combobox',{name:'Search commands'}).fill(`${mode} mode`);
      const option = page.locator(`#command-option-${mode}`);
      await option.waitFor({state:'visible'});
      const detail = await option.locator('small').innerText();
      check(`${label} ${mode} catalog agrees`,detail===dom.title && semantic(mode,detail),{title:dom.title,detail},'Command detail agrees with button capability description');
      check(`${label} ${mode} help opening preserves mode`,(await current())===before,await current(),before);
      await fs.writeFile(path.join(out,`${label}-${mode}-catalog.aria.txt`),await page.getByRole('dialog',{name:'Command directory'}).ariaSnapshot());
      await page.getByRole('combobox',{name:'Search commands'}).press('Escape');
      await page.getByRole('dialog',{name:'Command directory'}).waitFor({state:'hidden'});
    }
    for(let caseIndex=0;caseIndex<drafts.length;caseIndex++){
      const draft = drafts[caseIndex];await input.fill(draft);
      const before = await current();
      check(`${label} case ${caseIndex} input preserves mode`,(await current())===before,{mode:await current(),draftLength:draft.length},before);
      const sequence = Array.from({length:4},()=>modes[Math.floor(random()*modes.length)]);
      // Ensure each sequence includes a read-only transition and explicit exit.
      sequence[0]=caseIndex%2?'review':'plan';sequence[3]=caseIndex%2?'chat':'build';
      const trace={viewport:label,caseIndex,draft,sequence,observed:[]};
      for(const mode of sequence){
        await button(mode).click();
        const value = await input.inputValue();
        const observedMode = await current();
        trace.observed.push({mode:observedMode,draft:value});
        check(`${label} case ${caseIndex} switch ${trace.observed.length}`,value===draft && observedMode===mode,{mode:observedMode,draft:value},{mode,draft});
        report.boundedBudget.generatedSwitches++;
      }
      report.traces.push(trace);report.boundedBudget.generatedCases++;
    }
    // Slash commands have no arguments, so the real dispatcher cannot submit a task.
    for(const mode of modes){
      await input.fill(`/${mode}`);await input.press('Enter');
      check(`${label} slash /${mode} explicitly switches`,(await current())===mode,await current(),mode);
      await hint(mode,'slash');
    }
    await input.fill('/help');await input.press('Enter');
    await page.getByRole('dialog',{name:'Command directory'}).waitFor({state:'visible'});
    check(`${label} slash /help preserves mode`,(await current())==='build',await current(),'build');
    await page.getByRole('combobox',{name:'Search commands'}).press('Escape');
    const reloadDraft = 'Reload keeps mode: read-only review\nλ 🪶 café';
    await button('review').click();await input.fill(reloadDraft);
    await page.waitForTimeout(150);await page.reload({waitUntil:'networkidle'});
    await input.waitFor({state:'visible'});
    check(`${label} reload mode and draft`,(await current())==='review' && (await input.inputValue())===reloadDraft,{mode:await current(),draft:await input.inputValue()},{mode:'review',draft:reloadDraft});
    await hint('review','reload');await snapshot('reload-review');
    // Fresh client, same stored state, with no backend response replacement.
    const fresh = await activeContext.newPage();await fresh.goto(baseURL,{waitUntil:'networkidle'});
    await fresh.locator('#conversation-input').waitFor({state:'visible'});
    const freshMode = (await fresh.locator('.mode-switch button[aria-pressed="true"]').textContent()).trim();
    check(`${label} fresh page retained mode and draft`,freshMode==='review' && (await fresh.locator('#conversation-input').inputValue())===reloadDraft,{mode:freshMode,draft:await fresh.locator('#conversation-input').inputValue()},{mode:'review',draft:reloadDraft});
    await fresh.close();
    await cdp.detach();await activeContext.close();activeContext=null;
  }
  check('No AI task or external-effect action submitted',report.mutations.every(item=>item.command==='new-session'),report.mutations,'Only normal UI new-session creation if needed; no message/control/console action');
  check('No browser page errors',report.errors.length===0,report.errors,'No JavaScript exceptions during mode replay');
}catch(error){
  report.errors.push({type:'driver',message:error.stack||String(error)});
  report.scenarios.push({name:'Replay completion',status:'BLOCKED',expected:'All real UI cases executed',actual:error.message});
}finally{
  if(activeContext)await activeContext.close();
  await browser.close();
  report.finished=new Date().toISOString();
  report.summary=report.scenarios.reduce((counts,item)=>({...counts,[item.status]:(counts[item.status]||0)+1}),{});
  await fs.writeFile(path.join(out,'report.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify({summary:report.summary,budget:report.boundedBudget,pageErrors:report.errors,consoleCount:report.console.length,httpErrorCount:report.responses.length,mutations:report.mutations,output:out},null,2));
  if(report.scenarios.some(item=>item.status!=='PASS'))process.exitCode=1;
}
