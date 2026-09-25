import { chromium } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { randomUUID } from 'node:crypto';

const output=resolve('../.impeccable/review');
await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});
const observations=[];
const control=await browser.newContext();
const original=await (await control.request.get('http://127.0.0.1:4173/api/bootstrap')).json();
async function configure(command,args){
  const response=await control.request.post('http://127.0.0.1:4173/api/action',{headers:{'x-peritus-token':original.token},data:{command,operation:randomUUID(),...args}});
  const value=await response.json();if(!response.ok()||value.error)throw new Error(value.error??`Configuration failed (${response.status()})`);
}
let customized=false;
try {
// Demonstrate the reviewer's configurable-shortcut fix using a real saved binding.
await configure('preferences',{preferences:{...original.preferences,shortcuts:{...original.preferences.shortcuts,commands:'Mod+Shift+p'}}});
customized=true;
for(const [name,width,height]of [['desktop',1440,1000],['mobile',390,844]]){
  const context=await browser.newContext({viewport:{width,height},reducedMotion:'reduce'});
  const page=await context.newPage();
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto('http://127.0.0.1:4173');
  await page.getByRole('heading',{name:'A clear channel.' ,exact:false}).waitFor();
  await page.evaluate(()=>document.fonts.ready);
  await page.screenshot({path:resolve(output,`${name}.png`),fullPage:true});
  const accessibility=await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze();
  observations.push({name,errors,overflow:await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),violations:accessibility.violations.map(v=>({id:v.id,impact:v.impact,description:v.description,nodes:v.nodes.map(n=>({target:n.target,summary:n.failureSummary}))}))});
  if(name==='desktop'){
    observations.push({name:'configured-shortcut',binding:'Mod+Shift+p',visibleLegend:await page.locator('.command-key kbd').innerText(),restoredAfterCapture:true});
    await page.getByRole('button',{name:'Console settings',exact:true}).click();
    await page.getByRole('heading',{name:'Console configuration'}).waitFor();
    await page.screenshot({path:resolve(output,'settings.png'),fullPage:true});
    await page.getByRole('button',{name:'Close panel',exact:true}).click();
    await page.getByRole('button',{name:'Command',exact:false}).first().click();
    await page.getByRole('dialog',{name:'Command directory'}).waitFor();
    const search=page.getByRole('combobox',{name:'Search commands'});
    for(let index=0;index<16;index++)await search.press('ArrowDown');
    const selected=page.getByRole('option',{selected:true});
    await selected.waitFor();
    await page.screenshot({path:resolve(output,'commands.png'),fullPage:true});
    const paletteAccessibility=await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze();
    observations.push({name:'commands',activeDescendant:await search.getAttribute('aria-activedescendant'),selected:await selected.innerText(),selectionVisible:await selected.evaluate(node=>{const item=node.getBoundingClientRect(),list=node.parentElement.getBoundingClientRect();return item.top>=list.top&&item.bottom<=list.bottom;}),violations:paletteAccessibility.violations.map(v=>({id:v.id,description:v.description}))});
  }
  await context.close();
}
await writeFile(resolve(output,'inspection.json'),JSON.stringify(observations,null,2));
console.log(JSON.stringify(observations,null,2));
} finally {
  try { if(customized)await configure('config',{text:original.config}); }
  finally { await browser.close(); }
}
