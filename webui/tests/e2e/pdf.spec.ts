/// <reference types="node" />
import {test,expect,type Page} from '@playwright/test';
import {spawn,type ChildProcess} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,rm,symlink} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import AxeBuilder from '@axe-core/playwright';

// Full Chromium includes the real PDF extension; headless shell has no renderer.
test.use({channel:'chromium'});
let directory:string,root:string,project:string,token:string,pdf:Buffer,server:ChildProcess;
const filename='report #1.PDF',origin='http://127.0.0.1:4174';

test.beforeAll(async({browser,request})=>{
  directory=await mkdtemp('/tmp/opencode/peritus-pdf-e2e-');root=join(directory,'project');await mkdir(root);
  const document=await browser.newPage();
  await document.setContent('<!doctype html><html lang="en"><head><title>Peritus PDF fixture</title><style>body{font:18px sans-serif;padding:36px}section{break-after:page}section:last-child{break-after:auto}</style></head><body><section><h1>PDF preview regression</h1><p>Synthetic test document. This page must be visible inside Peritus.</p><p>Native browser rendering, not the UTF-8 editor.</p></section><section><h1>Second page</h1><p>Pagination remains available in the browser PDF controls.</p></section></body></html>');
  pdf=await document.pdf({format:'A4'});await document.close();
  await writeFile(join(root,filename),pdf);
  await writeFile(join(root,'broken.pdf'),'<html><script>throw new Error("must not run")</script>Not a PDF</html>');
  await writeFile(join(root,'notes.txt'),'Plain text remains editable.');
  await writeFile(join(directory,'outside.pdf'),pdf);await symlink(join(directory,'outside.pdf'),join(root,'escape.pdf'));
  server=spawn(resolve('../target/debug/peritus-web'),['--port','4174','--root',root,'--config',join(directory,'webui.toml'),'--state',join(directory,'workspace.json'),'--endpoint',join(directory,'absent.sock'),'--daemon-config',join(directory,'absent.toml'),'--product-state',join(directory,'product-state'),'--assets',resolve('dist')],{stdio:['ignore','pipe','pipe']});
  await new Promise<void>((done,reject)=>{let errors='';server.stderr!.on('data',value=>errors+=String(value));server.stdout!.on('data',value=>{if(String(value).includes('Peritus console:'))done();});server.once('error',reject);server.once('exit',code=>reject(new Error(`Gateway exited ${code}: ${errors}`)));});
  const boot=await(await request.get('/api/bootstrap')).json();token=boot.token;project=boot.workspace.projects[0].id;
});
test.afterAll(async()=>{
  if(server&&server.exitCode===null&&server.signalCode===null){server.kill('SIGINT');await new Promise<void>(done=>server.once('exit',()=>done()));}
  if(directory)await rm(directory,{recursive:true,force:true});
});
const url=(path=filename,kind='pdf')=>`${origin}/api/raw?${new URLSearchParams({project,path,kind})}`;
async function open(page:Page,path=filename){
  if((page.viewportSize()?.width??1440)<760)await page.getByRole('navigation',{name:'Workspace panels'}).getByRole('button',{name:'Files',exact:true}).click();
  await page.getByRole('region',{name:'Project file explorer'}).getByRole('button',{name:path,exact:true}).click();
}
async function rendered(page:Page){
  // Test-only introspection of the pinned browser's renderer: an iframe load
  // event alone does not establish that a PDF parsed or rendered successfully.
  await expect.poll(async()=>{
    const viewer=page.frames().find(frame=>frame.url().startsWith('chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/'));
    if(!viewer)return null;
    return viewer.evaluate(()=>{
      const element=document.querySelector('pdf-viewer') as (Element&{loadProgress_:number;docLength_:number})|null;
      return element?{progress:element.loadProgress_,pages:element.docLength_}:null;
    });
  }).toEqual({progress:100,pages:2});
}

test('PDF-only streaming preserves authentication, confinement, ranges and other file sandboxing',async({request})=>{
  const headers={'x-peritus-token':token};
  const response=await request.get(url(),{headers});expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toBe('application/pdf');expect(response.headers()['content-disposition']).toBe('inline');
  expect(response.headers()['content-security-policy']).toContain("frame-ancestors 'self'");expect(response.headers()['content-security-policy']).not.toContain('sandbox');
  expect(response.headers()['x-content-type-options']).toBe('nosniff');expect(await response.body()).toEqual(pdf);
  const range=await request.get(url(),{headers:{...headers,Range:'bytes=0-15'}});expect(range.status()).toBe(206);expect(await range.body()).toEqual(pdf.subarray(0,16));
  const head=await request.head(url(),{headers});expect(head.status()).toBe(200);expect(await head.body()).toHaveLength(0);
  expect((await request.get(url(),{headers:{...headers,Origin:'https://foreign.invalid'}})).status()).toBe(403);
  expect((await request.get(url(),{headers:{Cookie:'','x-peritus-token':''}})).status()).toBe(403);
  for(const path of ['escape.pdf','../outside.pdf','broken.pdf'])expect((await request.get(url(path),{headers})).status()).toBe(400);
  const ordinary=await request.get(url('broken.pdf','view'),{headers});expect(ordinary.headers()['content-security-policy']).toContain('sandbox');
  const download=await request.get(url(filename,'download'),{headers});expect(download.headers()['content-disposition']).toBe('attachment');
  expect((await request.get('/')).headers()['content-security-policy']).toContain("object-src 'none'");
});
test('a real two-page PDF renders inside the explorer file tab at desktop, phone and reported width',async({page})=>{
  const textRequests:string[]=[],errors:string[]=[];
  page.on('request',request=>{if(request.url().includes('/api/query?')){const params=new URL(request.url()).searchParams;if(params.get('kind')==='text'&&params.get('path')===filename)textRequests.push(request.url());}});
  page.on('pageerror',error=>errors.push(error.message));
  for(const width of [1440,390,1872]){
    await page.setViewportSize({width,height:1000});await page.goto('/');await open(page);await rendered(page);
    await expect(page.getByTitle(`PDF preview: ${filename}`,{exact:true})).toBeVisible();
    await expect(page.getByRole('link',{name:'Open PDF in new tab'})).toBeVisible();
    await expect(page.getByRole('button',{name:'Edit',exact:true})).toHaveCount(0);
    await expect(page.getByText(/UTF-8 ·/)).toHaveCount(0);expect(textRequests).toEqual([]);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth)).toBe(false);
    await page.screenshot({path:resolve(`../.impeccable/review/pdf-${width}.png`),fullPage:true});
  }
  expect(errors).toEqual([]);
});
test('switching away from pending text cannot strand PDF or media loading',async({page})=>{
  let started!:()=>void,release!:()=>void;
  const pending=new Promise<void>(done=>started=done),gate=new Promise<void>(done=>release=done);
  await page.route('**/api/query?**',async route=>{const params=new URL(route.request().url()).searchParams;if(params.get('kind')==='text'&&params.get('path')==='notes.txt'){started();await gate;}await route.continue();});
  await page.goto('/');await open(page,'notes.txt');await pending;
  try{await open(page);await rendered(page);await expect(page.getByLabel('Loading file',{exact:true})).toHaveCount(0);}
  finally{release();}
  await page.getByRole('button',{name:'notes.txt',exact:true}).last().click();
  await expect(page.getByRole('button',{name:'Edit',exact:true})).toBeVisible();
});
test('disabled native viewing has an accessible open/download fallback',async({page})=>{
  await page.addInitScript(()=>Object.defineProperty(Navigator.prototype,'pdfViewerEnabled',{get:()=>false}));
  for(const width of [1440,390,1872]){
    await page.setViewportSize({width,height:1000});await page.goto('/');await open(page);
    await expect(page.getByRole('heading',{name:'Inline PDF viewing is unavailable'})).toBeVisible();
    await expect(page.locator('iframe')).toHaveCount(0);await expect(page.getByRole('link',{name:'Download',exact:true})).toBeVisible();
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa']).analyze()).violations).toEqual([]);
    await page.screenshot({path:resolve(`../.impeccable/review/pdf-fallback-${width}.png`),fullPage:true});
  }
});
test('PDF preflight failures are actionable and retry can recover',async({page})=>{
  await page.goto('/');await open(page,'broken.pdf');
  await expect(page.getByRole('heading',{name:'PDF preview unavailable'})).toBeVisible();await expect(page.getByText('No PDF header was found.',{exact:false})).toBeVisible();
  await expect(page.locator('iframe')).toHaveCount(0);
  await writeFile(join(root,'broken.pdf'),pdf);await page.getByRole('button',{name:'Retry PDF preview'}).click();await rendered(page);
});
