/// <reference types="node" />
import { test, expect, type APIRequestContext } from '@playwright/test';
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, rm, symlink } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { randomUUID } from 'node:crypto';
import AxeBuilder from '@axe-core/playwright';

let temporary:string,root:string,repository:string,other:string,remote:string,server:ChildProcess,token:string,project:string,firstSession:string;
const origin='http://127.0.0.1:4174';
function git(cwd:string,...args:string[]){return execFileSync('git',args,{cwd,encoding:'utf8',stdio:['ignore','pipe','pipe']});}
async function startServer(){
  server=spawn(resolve('../target/debug/peritus-web'),['--port','4174','--root',root,'--config',join(temporary,'webui.toml'),'--state',join(temporary,'workspace.json'),'--daemon-config',join(temporary,'unconfigured.toml'),'--endpoint',join(temporary,'absent.sock'),'--product-state',join(temporary,'product-state'),'--assets',resolve('dist')],{stdio:['ignore','pipe','pipe']});
  await new Promise<void>((done,reject)=>{let errors='';server.stderr!.on('data',chunk=>errors+=String(chunk));server.stdout!.on('data',chunk=>{if(String(chunk).includes('Peritus console:'))done();});server.once('error',reject);server.once('exit',code=>reject(new Error(`gateway exited ${code}: ${errors}`)));});
}
async function stopServer(){if(server&&server.exitCode===null&&server.signalCode===null){server.kill('SIGINT');await new Promise<void>(done=>server.once('exit',()=>done()));}}
async function action(request:APIRequestContext,command:string,args:Record<string,unknown>={},operation=randomUUID()){
  const response=await request.post(`${origin}/api/action`,{headers:{'x-peritus-token':token},data:{command,operation,...args}});
  return response.json();
}
async function query(request:APIRequestContext,kind:string,args:Record<string,string>={}){
  const response=await request.get(`${origin}/api/query`,{headers:{'x-peritus-token':token},params:{kind,project,...args}});return response.json();
}
test.describe.configure({mode:'serial'});
test.beforeAll(async({request})=>{
  temporary=await mkdtemp('/tmp/opencode/peritus-console-e2e-');root=join(temporary,'alpha');repository=join(root,'source');other=join(temporary,'beta');remote=join(temporary,'remote.git');
  await mkdir(repository,{recursive:true});await mkdir(other);
  await writeFile(join(root,'README.md'),'# Test project\n\nA **real** local file.\n');
  await writeFile(join(root,'sample.py'),'def hello():\n    return "peritus"\n');
  await writeFile(join(root,'edit-me.md'),'\ufeff# Quick tweaks\r\n\r\nA typo and another typo.\r\n');
  await writeFile(join(root,'large.txt'),'Large preview line\n'.repeat(200_000));
  await writeFile(join(root,'image.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><rect width="100" height="80" fill="#eaa267"/></svg>');
  await writeFile(join(root,'binary.dat'),Buffer.from([0,1,2,3]));
  await symlink(other,join(root,'outside'));
  git(repository,'init','-b','main');git(repository,'config','user.name','Console Test');git(repository,'config','user.email','console@example.invalid');
  await writeFile(join(repository,'code.ts'),'export const value = 1;\n');await writeFile(join(repository,'.gitignore'),'# Keep this line\n');
  git(repository,'add','.');git(repository,'commit','-m','initial');git(temporary,'init','--bare',remote);git(repository,'remote','add','origin',remote);git(repository,'push','-u','origin','main');
  await startServer();
  const boot=await (await request.get(`${origin}/api/bootstrap`)).json();token=boot.token;project=boot.workspace.projects[0].id;firstSession=boot.workspace.sessions[0].id;
});
test.afterAll(async()=>{await stopServer();if(temporary)await rm(temporary,{recursive:true,force:true});});

test('gateway confines files and rejects foreign origins',async({request})=>{
  const denied=await request.post('/api/action',{headers:{Origin:'https://other.example'},data:{operation:randomUUID(),command:'open-project',root:other}});expect(denied.status()).toBe(403);
  expect((await query(request,'text',{path:'../beta/secret'})).error).toBeTruthy();
  expect((await query(request,'files',{path:'outside'})).error).toContain('outside');
  expect((await query(request,'text',{path:'sample.py'})).text).toContain('def hello');
  expect((await query(request,'text',{path:'binary.dat'})).error).toContain('Binary');
  const raw=await request.get('/api/raw',{params:{project,path:'sample.py'},headers:{'x-peritus-token':token,Range:'bytes=0-2'}});expect(raw.status()).toBe(206);expect(await raw.text()).toBe('def');
});
test('nested tabs are canonical-root-bound, cycle-free, and idempotent',async({request})=>{
  const operation=randomUUID();const args={project,parent:firstSession,title:'Nested review'};
  const child=await action(request,'new-session',args,operation);const repeated=await action(request,'new-session',args,operation);expect(repeated.id).toBe(child.id);
  expect((await action(request,'session',{session:firstSession,parent:child.id})).error).toContain('descendants');
  const second=await action(request,'open-project',{root:other});
  expect((await action(request,'new-session',{project:second.id,parent:firstSession})).error).toContain('same canonical project root');
  const alias=join(temporary,'alias');await symlink(root,alias);expect((await action(request,'open-project',{root:alias})).id).toBe(project);
  expect((await action(request,'new-session',{...args,title:'Different'},operation)).error).toContain('different input');
});
test('explicit nested repository supports stage, commit, push, pull, and literal ignore',async({request})=>{
  expect((await query(request,'git')).error).toContain('No working Git');
  await action(request,'repository',{project,path:root});
  await action(request,'repository',{project,path:repository});
  expect((await query(request,'git')).branch).toBe('main');
  await writeFile(join(repository,'code.ts'),'export const value = 2;\n');
  expect((await query(request,'git')).changes).toEqual(expect.arrayContaining([expect.objectContaining({path:'code.ts'})]));
  await action(request,'git',{project,action:'add',paths:['code.ts']});
  expect((await query(request,'git')).changes[0].code).toBe('M ');
  expect((await action(request,'git',{project,action:'commit',message:'native console commit'})).error).toBeUndefined();
  expect(git(repository,'log','-1','--format=%s').trim()).toBe('native console commit');
  expect((await action(request,'git',{project,action:'push'})).error).toBeUndefined();
  const clone=join(temporary,'clone');git(temporary,'clone','-b','main',remote,clone);git(clone,'config','user.name','Remote');git(clone,'config','user.email','remote@example.invalid');await writeFile(join(clone,'remote.txt'),'pulled\n');git(clone,'add','.');git(clone,'commit','-m','remote change');git(clone,'push');
  expect((await action(request,'git',{project,action:'pull'})).error).toBeUndefined();expect(await readFile(join(repository,'remote.txt'),'utf8')).toBe('pulled\n');
  await writeFile(join(repository,'literal[1]*.txt'),'ignore me');const ignored=await action(request,'ignore',{project,path:'source/literal[1]*.txt'});expect(ignored.pattern).toBe('/literal\\[1\\]\\*.txt');
  expect(await readFile(join(repository,'.gitignore'),'utf8')).toContain('# Keep this line\n');expect(git(repository,'check-ignore','--','literal[1]*.txt').trim()).toBe('literal[1]*.txt');
});
test('mouse and keyboard open files, nest tabs, and retain drafts across reload',async({page})=>{
  await page.goto('/');await page.getByRole('heading',{name:'New conversation',exact:true}).waitFor();
  await page.getByRole('button',{name:'README.md',exact:true}).click();await expect(page.locator('.viewer-content h1')).toHaveText('Test project');
  await page.getByRole('button',{name:'Conversation',exact:true}).click();
  await page.getByRole('textbox',{name:'Message Peritus or enter a slash command'}).fill('Retain this unsent draft');await page.reload();
  await expect(page.getByRole('textbox',{name:'Message Peritus or enter a slash command'})).toHaveValue('Retain this unsent draft');
  await page.keyboard.press('Control+k');await expect(page.getByRole('dialog',{name:'Command directory'})).toBeVisible();
  const search=page.getByRole('combobox',{name:'Search commands'});
  for(let index=0;index<16;index++)await search.press('ArrowDown');
  const selected=page.getByRole('option',{selected:true});
  await expect(search).toHaveAttribute('aria-activedescendant',(await selected.getAttribute('id'))!);
  await expect(search).toBeFocused();
  await expect.poll(()=>selected.evaluate(node=>{const item=node.getBoundingClientRect(),list=node.parentElement!.getBoundingClientRect();return item.top>=list.top&&item.bottom<=list.bottom;})).toBe(true);
  expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  const previousTab=await page.locator('.session-tab.current button[role="tab"]').getAttribute('id');
  await search.fill('Nest a conversation');await search.press('Enter');
  await expect(page.getByRole('tablist',{name:'Nested sessions level 1'})).toBeVisible();
  const active=page.locator('.session-tab.current').last();
  // The nested row already contains another fixture session. Wait for this
  // asynchronous creation, not merely for that pre-existing row to be visible.
  await expect(active.getByRole('tab')).not.toHaveAttribute('id',previousTab!);
  const title=await active.locator('.session-tab-title').innerText();
  await page.locator('.session-level').filter({has:active}).getByRole('button',{name:`Close session: ${title}`,exact:true}).click();await page.getByRole('button',{name:'Session library',exact:false}).last().click();await expect(page.getByRole('dialog',{name:'Session library'})).toBeVisible();
});
test('text editor saves exact line endings, retains tab drafts, supports undo and rejects external conflicts',async({page})=>{
  await page.goto('/');await page.getByRole('button',{name:'edit-me.md',exact:true}).click();
  await expect(page.getByRole('button',{name:'View',exact:true})).toHaveAttribute('aria-pressed','true');
  await page.getByRole('button',{name:'Edit',exact:true}).click();
  const editor=page.getByRole('textbox',{name:'Edit edit-me.md',exact:true});
  await editor.press('Control+f');await page.getByRole('textbox',{name:'Find text',exact:true}).fill('typo');
  await page.getByRole('textbox',{name:'Replace with',exact:true}).fill('correction');await page.getByRole('button',{name:'Replace all',exact:true}).click();
  await expect(editor).toHaveValue('# Quick tweaks\n\nA correction and another correction.\n');
  await page.getByRole('button',{name:'Undo',exact:true}).click();
  await expect(editor).toHaveValue('# Quick tweaks\n\nA typo and another typo.\n');
  await editor.press('Control+Shift+z');await expect(editor).toHaveValue('# Quick tweaks\n\nA correction and another correction.\n');
  await page.getByRole('button',{name:'Conversation',exact:true}).click();await page.locator('.content-file-tab').getByRole('button',{name:'edit-me.md',exact:true}).click();
  await expect(editor).toHaveValue('# Quick tweaks\n\nA correction and another correction.\n');
  await editor.press('Control+s');await expect(page.getByRole('button',{name:'Save',exact:true})).toBeDisabled();
  expect(await readFile(join(root,'edit-me.md'),'utf8')).toBe('\ufeff# Quick tweaks\r\n\r\nA correction and another correction.\r\n');
  await editor.fill('My unsaved change\n');await writeFile(join(root,'edit-me.md'),'External disk change\n');
  await editor.press('Control+s');await expect(page.getByRole('alert').filter({hasText:'changed on disk'})).toBeVisible();
  await expect(editor).toHaveValue('My unsaved change\n');expect(await readFile(join(root,'edit-me.md'),'utf8')).toBe('External disk change\n');
  for(const width of [1440,390]){
    await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    await page.screenshot({path:resolve(`../.impeccable/review/editor-${width}.png`),fullPage:true});
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  }
  page.once('dialog',dialog=>dialog.accept());await page.getByRole('button',{name:'Reload disk version',exact:true}).click();
  await expect(page.getByRole('button',{name:'View',exact:true})).toHaveAttribute('aria-pressed','true');
});
test('large previews are paged and file saves can exceed the old 4 MiB body limit',async({request,page})=>{
  await page.goto('/');await page.getByRole('button',{name:'large.txt',exact:true}).click();
  await expect(page.getByText('Large file · plain-text pages')).toBeVisible();
  expect((await page.getByRole('region',{name:'File source',exact:true}).innerText()).length).toBeLessThanOrEqual(128_001);
  await page.getByRole('button',{name:'Next page',exact:true}).click();await expect(page.getByRole('spinbutton',{name:'Preview page'})).toHaveValue('2');
  const original=await query(request,'text',{path:'large.txt'}),content='z'.repeat(5*1024*1024);
  const saved=await request.put('/api/file',{headers:{'x-peritus-token':token,'Content-Type':'text/plain'},params:{project,path:'large.txt',revision:original.revision,operation:randomUUID()},data:content});
  expect((await saved.json()).bytes).toBe(content.length);expect((await readFile(join(root,'large.txt'))).length).toBe(content.length);
});
test('explorer preview and drag attachment are distinct, session-bound and persistent',async({request,page})=>{
  await page.goto('/');await page.getByRole('button',{name:'sample.py',exact:true}).click();
  await expect(page.getByRole('region',{name:'File viewer: sample.py'})).toBeVisible();
  await page.getByRole('button',{name:'Conversation',exact:true}).click();await expect(page.getByLabel('Attachments for next message')).toHaveCount(0);
  await page.locator('.file-tree .file-label').filter({hasText:'sample.py'}).dragTo(page.getByRole('textbox',{name:'Message Peritus or enter a slash command'}));
  await expect(page.getByLabel('Attachments for next message')).toContainText('sample.py');
  await page.getByRole('textbox',{name:'Message Peritus or enter a slash command'}).fill('Explain this file');
  await expect(page.getByRole('button',{name:'Send message',exact:true})).toBeDisabled();
  await page.reload();await expect(page.getByLabel('Attachments for next message')).toContainText('sample.py');
  const retained=JSON.parse(await readFile(join(temporary,'workspace.json'),'utf8'));
  const attachment=Object.values(retained.attachments).find((file:any)=>file.path==='sample.py') as {id:string;session:string};
  expect(await readFile(join(temporary,'attachments',attachment.id),'utf8')).toContain('def hello');
  for(const width of [1440,390]){
    await page.setViewportSize({width,height:900});await page.screenshot({path:resolve(`../.impeccable/review/attachments-${width}.png`),fullPage:true});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  }
  await page.getByRole('button',{name:'Remove attachment sample.py',exact:true}).click();await expect(page.getByLabel('Attachments for next message')).toHaveCount(0);
  expect((await action(request,'attach-file',{session:firstSession,project,path:'binary.dat'})).error).toContain('UTF-8');
});
test('branch and remote forms operate on the selected repository',async({page})=>{
  await page.goto('/');await page.locator('.drawer-switch').getByRole('button',{name:'Git',exact:false}).click();
  await page.locator('summary').filter({hasText:'Branches'}).click();await page.getByRole('textbox',{name:'New branch',exact:true}).fill('ui-workflow');await page.getByRole('button',{name:'Create & switch',exact:true}).click();
  await expect(page.locator('.git-location strong')).toHaveText('ui-workflow');
  await page.locator('.git-workflow summary').filter({hasText:'Remotes'}).click();await page.getByLabel('Remote',{exact:true}).selectOption('origin');
  await page.locator('.git-workflow summary').filter({hasText:'Branches'}).click();
  await page.getByRole('button',{name:'Dismiss notification'}).click();
  for(const width of [1440,390]){
    await page.setViewportSize({width,height:900});if(width===390)await page.getByRole('navigation',{name:'Workspace panels'}).getByRole('button',{name:'Files',exact:true}).click();
    await page.locator('.git-scroll').evaluate(node=>node.scrollTop=0);
    await page.screenshot({path:resolve(`../.impeccable/review/git-remotes-${width}.png`),fullPage:true});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  }
  await page.locator('.git-workflow summary').filter({hasText:'Remotes'}).click();await page.locator('.git-workflow summary').filter({hasText:'Branches'}).click();
  for(const width of [1440,390]){await page.setViewportSize({width,height:900});await page.locator('.git-scroll').evaluate(node=>node.scrollTop=0);await page.screenshot({path:resolve(`../.impeccable/review/git-branches-${width}.png`),fullPage:true});}
  await page.getByLabel('Switch branch',{exact:true}).selectOption('refs/heads/main');await page.getByRole('button',{name:'Switch branch',exact:true}).click();await expect(page.locator('.git-location strong')).toHaveText('main');
});
test('each session tab closes after its directory is deleted, including nested and inactive tabs',async({request,page})=>{
  for(const width of [1440,390]){
    const deleted=join(temporary,`deleted-project-${width}`);await mkdir(deleted);
    const opened=await action(request,'open-project',{root:deleted});
    const bootstrap=await (await request.get('/api/bootstrap')).json();
    const parent=bootstrap.workspace.sessions.find((session:{project:string})=>session.project===opened.id);
    await action(request,'session',{session:parent.id,title:'Parent session'});
    const child=await action(request,'new-session',{project:opened.id,parent:parent.id,title:'Nested session'});
    const sibling=await action(request,'new-session',{project:opened.id,title:'Sibling session'});
    await page.setViewportSize({width,height:width===390?844:1000});await page.goto('/');
    await page.locator('.project-tab').filter({hasText:`deleted-project-${width}`}).click();
    await page.getByRole('tab',{name:'Nested session',exact:true}).click();
    await rm(deleted,{recursive:true});await page.reload();
    await expect(page.getByRole('tab',{name:'Nested session',exact:true})).toHaveAttribute('aria-selected','true');
    for(const title of ['Parent session','Sibling session','Nested session']){
      const label=await page.getByRole('tab',{name:title,exact:true}).locator('.session-tab-title').boundingBox();
      const close=await page.getByRole('button',{name:`Close session: ${title}`,exact:true}).boundingBox();
      expect(label!.x+label!.width).toBeLessThanOrEqual(close!.x);
    }
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    await page.screenshot({path:resolve(`../.impeccable/review/session-close-${width}.png`),fullPage:true});
    for(const title of ['Parent session','Sibling session']){
      await page.getByRole('button',{name:`Close session: ${title}`,exact:true}).click();
      await expect(page.getByRole('tab',{name:title,exact:true})).toHaveCount(0);
      await expect(page.getByRole('tab',{name:'Nested session',exact:true})).toHaveAttribute('aria-selected','true');
    }
    await page.getByRole('button',{name:'Close session: Nested session',exact:true}).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('tab')).toHaveCount(0);
    await expect(page.getByRole('button',{name:'Add session',exact:true})).toBeFocused();
    await page.reload();await expect(page.getByRole('heading',{name:'Your work is still here.'})).toBeVisible();
    const retained=JSON.parse(await readFile(join(temporary,'workspace.json'),'utf8'));
    for(const id of [parent.id,child.id,sibling.id])expect(retained.sessions.find((session:{id:string})=>session.id===id).closed).toBe(true);
    expect(retained.sessions.find((session:{id:string})=>session.id===child.id).parent).toBe(parent.id);
    expect((await action(request,'new-session',{project:opened.id})).error).toBeTruthy();
    const reopened=await action(request,'session',{session:child.id,closed:false});expect(reopened.closed).toBe(false);
    await action(request,'session',{session:child.id,closed:true});
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  }
});
test('gateway restart rotates auth and preserves an unresolved operation without repeating it',async({request,page})=>{
  // Earlier scenarios leave multiple sessions. Recovery belongs to its exact
  // target, not whichever session sorts first in a fresh browser's bootstrap.
  await page.addInitScript(({project,firstSession})=>{if(!localStorage.getItem('peritus:layout:v1'))localStorage.setItem('peritus:layout:v1',JSON.stringify({projectId:project,sessionId:firstSession}));},{project,firstSession});
  await page.goto('/');await expect(page.locator(`#session-${firstSession}`)).toHaveAttribute('aria-selected','true');
  const composer=page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});await composer.fill('Keep the original draft');
  const oldToken=token,operation=randomUUID();await stopServer();
  const state=JSON.parse(await readFile(join(temporary,'workspace.json'),'utf8'));
  state.operations[operation]={input:{operation,command:'send',session:firstSession,text:'Keep the original draft'},result:null};
  await writeFile(join(temporary,'workspace.json'),JSON.stringify(state));await startServer();
  expect((await request.get('/api/query',{headers:{'x-peritus-token':oldToken},params:{kind:'files',project}})).status()).toBe(403);
  const boot=await (await request.get('/api/bootstrap')).json();token=boot.token;expect(token).not.toBe(oldToken);
  await page.reload();await expect(page.getByText('Unresolved send operation',{exact:true})).toBeVisible();await expect(composer).toHaveValue('Keep the original draft');
  await page.getByRole('button',{name:'Check original outcome'}).click();
  await expect(page.getByText('Unresolved send operation',{exact:true})).toBeVisible();
  await expect(page.getByText('Still unresolved.',{exact:false})).toBeVisible();
  expect((await action(request,'send',{session:firstSession,text:'Keep the original draft'},operation)).uncertain).toBe(true);
  expect((await action(request,'send',{session:firstSession,text:'A duplicate attempt'})).error).toContain('Resolve original operation');
  for(const width of [1440,390]){await page.setViewportSize({width,height:900});await page.screenshot({path:resolve(`../.impeccable/review/recovery-${width}.png`),fullPage:true});expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);}
  // Synthetic HTTP admission facts isolate the healthy-but-held presentation.
  // The original operation and all recovery effects still use the real gateway.
  let checkMode='normal';
  await page.route('**/api/query?**',async route=>{
    const kind=new URL(route.request().url()).searchParams.get('kind');
    if(kind==='daemon')return route.fulfill({json:{connected:true,ready:true,readiness:'ReadyReadWrite'}});
    if(kind==='facts')return route.fulfill({json:{ready:true,reason:'Synthetic admitted workspace fixture',workspace:{id:'fixture',root,execution:root,trust:'trusted'},providers:[],endpoint:'isolated-fixture'}});
    if(kind==='operation'&&checkMode==='failed')return route.fulfill({status:503,json:{error:'Synthetic receipt query failure'}});
    if(kind==='operation'&&checkMode==='slow')await new Promise(resolve=>setTimeout(resolve,800));
    return route.continue();
  });
  await page.setViewportSize({width:1440,height:900});await page.reload();
  await expect(page.getByText('Daemon ready',{exact:true})).toBeVisible();
  await expect(page.locator('.connection-banner')).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Send message'})).toBeDisabled();
  await composer.fill('');await expect(page.getByText('RECOVERY HOLD',{exact:true})).toBeVisible();await composer.fill('Keep the original draft');
  checkMode='slow';await page.getByRole('button',{name:'Check original outcome'}).click();
  await expect(page.getByRole('button',{name:'Checking…',exact:true})).toBeDisabled();
  await expect(page.getByText('Checking the original outcome…',{exact:true})).toBeVisible();
  await expect(page.getByText('Still unresolved.',{exact:false})).toBeVisible();
  for(const width of [1440,390]){
    await page.setViewportSize({width,height:900});await page.screenshot({path:resolve(`../.impeccable/review/recovery-ready-${width}.png`),fullPage:true});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
  }
  checkMode='failed';await page.getByRole('button',{name:'Check original outcome'}).click();
  await expect(page.getByText('Could not check the original outcome.',{exact:false})).toBeVisible();
  await expect(page.getByRole('button',{name:'Send message'})).toBeDisabled();
  for(const width of [1440,390]){await page.setViewportSize({width,height:900});await page.screenshot({path:resolve(`../.impeccable/review/recovery-check-failed-${width}.png`),fullPage:true});}
  checkMode='normal';
  page.once('dialog',dialog=>dialog.accept());await page.getByRole('button',{name:'I inspected the outcome'}).click();await expect(page.getByText('Unresolved send operation',{exact:true})).toHaveCount(0);
  const receipt=await query(request,'operation',{operation});expect(receipt.result.reviewed).toBe(true);expect(receipt.result.error).toContain('not repeated');await expect(composer).toHaveValue('Keep the original draft');
});
test('configuration validates, persists behavioral settings, and aliases share the dispatcher',async({request,page})=>{
  const bad=await action(request,'config',{text:'font_size = 99\ntheme = "nixie"'});expect(bad.error).toContain('12–22');
  await action(request,'config',{text:'theme = "daylight"\nmotion = false\nword_wrap = false\n[aliases]\nbranch = "/nest"\n[shortcuts]\ncommands = "Mod+Shift+p"\n'});
  await page.goto('/');await expect(page.locator('html')).toHaveAttribute('data-theme','daylight');
  await expect(page.locator('.command-key kbd')).toHaveText(/^(Ctrl|Cmd) Shift P$/);
  await page.keyboard.press('Control+Shift+p');await expect(page.getByRole('dialog',{name:'Command directory'})).toBeVisible();await page.keyboard.press('Escape');
  await page.getByRole('textbox',{name:'Message Peritus or enter a slash command'}).fill('/branch');await page.keyboard.press('Enter');await expect(page.getByRole('tablist',{name:'Nested sessions level 1'})).toBeVisible();
  const config=await readFile(join(temporary,'webui.toml'),'utf8');expect(config).toContain('word_wrap = false');
  await action(request,'config',{text:'theme = "nixie"\nmotion = false\n'});
});
test('retained CLI PTY really executes the installed CLI',async({request})=>{
  const console=await action(request,'console',{project,args:['--version']});expect(console.id).toBeTruthy();
  await expect.poll(async()=>{const value=await (await request.get(`/api/terminal/${console.id}`,{headers:{'x-peritus-token':token}})).json();return Buffer.from(value.data,'base64').toString();}).toContain('peritus');
  await action(request,'close-console',{id:console.id});
});
test('desktop and phone expose named, contrast-checked controls without horizontal overflow',async({page})=>{
  for(const width of [1440,390]){
    await page.setViewportSize({width,height:900});await page.goto('/');await page.getByRole('textbox',{name:'Message Peritus or enter a slash command'}).waitFor();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    const results=await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze();expect(results.violations).toEqual([]);
  }
});
test('project Xs preserve sessions, work with missing folders, and allow closing every project',async({request,page})=>{
  for(const width of [1440,390]){
    const directory=join(temporary,`close-project-${width}`);await mkdir(directory);
    const opened=await action(request,'open-project',{root:directory});
    const boot=await (await request.get('/api/bootstrap')).json();
    const parent=boot.workspace.sessions.find((s:{project:string})=>s.project===opened.id);
    const title=`Retained project session ${width}`;
    await action(request,'session',{session:parent.id,title});
    const child=await action(request,'new-session',{project:opened.id,parent:parent.id,title:'Retained child'});
    await page.setViewportSize({width,height:900});await page.goto('/');
    await page.getByRole('button',{name:`Close project: close-project-${width}`,exact:true}).click();
    await expect(page.locator(`#project-${opened.id}`)).toHaveCount(0);
    expect((await action(request,'open-project',{root:directory})).id).toBe(opened.id);
    await page.reload();await page.locator(`#project-${opened.id}`).click();
    await page.screenshot({path:resolve(`../.impeccable/review/project-close-${width}.png`),fullPage:true});
    await rm(directory,{recursive:true});
    await page.getByRole('button',{name:`Close project: close-project-${width}`,exact:true}).focus();await page.keyboard.press('Enter');
    await expect(page.locator(`#project-${opened.id}`)).toHaveCount(0);
    await page.reload();await expect(page.locator(`#project-${opened.id}`)).toHaveCount(0);
    await page.locator('.command-key').click();
    await page.getByRole('combobox',{name:'Search commands'}).fill('Session library');
    await page.getByRole('combobox',{name:'Search commands'}).press('Enter');
    await page.locator('.library-row').filter({hasText:title}).getByRole('button',{name:'Open',exact:true}).click();
    await expect(page.locator(`#project-${opened.id}`)).toHaveAttribute('aria-current','page');
    await expect(page.getByRole('tab',{name:'Retained child',exact:true})).toBeVisible();
    while(await page.locator('.project-close').count()){
      const count=await page.locator('.project-close').count();
      await page.locator('.project-close').last().click();
      await expect(page.locator('.project-close')).toHaveCount(count-1);
    }
    await expect(page.locator('.add-project')).toBeFocused();
    await page.reload();await expect(page.getByRole('heading',{name:'Open a project to get started.'})).toBeVisible();
    await expect(page.locator('.project-tab')).toHaveCount(0);await expect(page.locator('.file-tree .file-row')).toHaveCount(0);
    const retained=JSON.parse(await readFile(join(temporary,'workspace.json'),'utf8'));
    expect(retained.projects.every((p:{closed:boolean})=>p.closed)).toBe(true);
    expect(retained.sessions.find((s:{id:string})=>s.id===child.id).parent).toBe(parent.id);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
    await action(request,'open-project',{root});
  }
});
