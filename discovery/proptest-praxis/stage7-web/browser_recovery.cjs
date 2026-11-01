#!/usr/bin/env node
/* Exercise the built Svelte composer in Chromium with a scheduled API failure. */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require(path.resolve(__dirname, '../../../webui/node_modules/playwright'));

const original = 'Shared https://doc.qt.io for reading.';
const edited = ' Shared https://doc.qt.io for reading. ';
const project = {id:'p1',root:'/tmp/peritus-web-property',name:'Property project',repository:'/tmp/peritus-web-property',closed:false};
const session = {id:'s1',conversation:'c1',run:'',project:'p1',parent:null,title:'Existing',closed:false,settings:{models:{},providers:{}}};
const facts = {ready:true,reason:'',providers:[],workspace:{id:'w1',root:project.root,execution:'',trust:''},endpoint:''};
const result = {received:'9007199254740993',activities:[{id:'9007199254740993',kind:'user',text:original,detail:''}]};
const payload = value => ({status:200,contentType:'application/json',body:JSON.stringify(value)});
async function main() {
  let releaseAction, actionSeen;
  const seen = new Promise(resolve => {actionSeen = resolve;});
  const release = new Promise(resolve => {releaseAction = resolve;});
  let actionCount = 0, checks = 0, sent;
  const browser = await chromium.launch({executablePath:'/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
  try {
    const page = await browser.newPage();
    await page.route('**/api/**', async route => {
      const url = new URL(route.request().url());
      if(url.pathname === '/api/bootstrap') return route.fulfill(payload({workspace:{projects:[project],sessions:[session]},preferences:null,config:'',configError:null,configPath:'',token:'test-token',pendingOperations:[]}));
      if(url.pathname === '/api/action') {
        actionCount++; sent = route.request().postDataJSON(); actionSeen();
        await release; return route.abort('failed');
      }
      if(url.pathname === '/api/query') {
        const kind = url.searchParams.get('kind');
        if(kind === 'operation') {checks++;return route.fulfill(payload(checks===1?null:{input:{text:sent.text,attachments:[]},result}));}
        const values = {daemon:{ready:true,readiness:'Ready'},consoles:[],facts,git:{root:project.root,branch:'main',changes:[],remotes:'',branches:[],remoteDetails:[]},conversation:{activities:[]},files:{inventory:'0',entries:[],total:0,next:null}};
        if(kind in values)return route.fulfill(payload(values[kind]));
      }
      throw new Error(`Unexpected request ${url}`);
    });
    await page.goto('http://127.0.0.1:5187/');
    const composer = page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});
    await composer.waitFor({state:'visible'});
    await composer.fill(original);
    await page.getByRole('button',{name:'Send message'}).click();
    await seen;
    await composer.fill(edited);
    assert.equal(await composer.inputValue(),edited,'edit must be visible while original send is pending');
    releaseAction();
    await page.getByText('Unresolved send operation').waitFor({state:'visible'});
    assert.equal(await composer.inputValue(),edited,'uncertain outcome must retain visible edit');
    await page.getByRole('button',{name:'Check original outcome'}).click();
    await page.getByText('Unresolved send operation').waitFor({state:'hidden'});
    assert.equal(await composer.inputValue(),edited,'recovered send must retain visible edit');
    assert.equal(actionCount,1,'failed mutation was not repeated');
    assert.equal(checks,2,'manual check queried the original receipt');
    assert.equal(sent.text,original);
    const report={status:'pass',browser:'Google Chrome headless',scenario:'send, edit while pending, connection loss, receipt recovery',original,edited,visibleDraft:await composer.inputValue(),actionCount,checks};
    const outputIndex=process.argv.indexOf('--output');
    if(outputIndex>=0){const output=path.resolve(process.argv[outputIndex+1]);fs.mkdirSync(output,{recursive:true});fs.writeFileSync(path.join(output,'browser-report.json'),JSON.stringify(report,null,2)+'\n');}
    console.log(JSON.stringify(report));
  } finally {await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
