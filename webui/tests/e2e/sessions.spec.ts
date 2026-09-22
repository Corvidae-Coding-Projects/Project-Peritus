/// <reference types="node" />
import { test, expect, type APIRequestContext } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';

let temporary:string,root:string,server:ChildProcess,token:string,project:string,session:string;
const origin='http://127.0.0.1:4174';
async function startServer(){
  server=spawn(resolve('../target/debug/peritus-web'),['--port','4174','--root',root,'--config',join(temporary,'webui.toml'),'--state',join(temporary,'workspace.json'),'--daemon-config',join(temporary,'absent.toml'),'--endpoint',join(temporary,'absent.sock'),'--product-state',join(temporary,'absent-state'),'--assets',resolve('dist'),'--cli',join(temporary,'cli')],{stdio:['ignore','pipe','pipe']});
  await new Promise<void>((done,reject)=>{let errors='';server.stderr!.on('data',chunk=>errors+=String(chunk));server.stdout!.on('data',chunk=>{if(String(chunk).includes('Peritus console:'))done();});server.once('error',reject);server.once('exit',code=>reject(new Error(`gateway exited ${code}: ${errors}`)));});
}
async function stopServer(){if(server&&server.exitCode===null&&server.signalCode===null){server.kill('SIGINT');await new Promise<void>(done=>server.once('exit',()=>done()));}}
async function action(request:APIRequestContext,command:string,args:Record<string,unknown>={}){
  return (await request.post(`${origin}/api/action`,{headers:{'x-peritus-token':token},data:{command,operation:randomUUID(),...args}})).json();
}
test.describe.configure({mode:'serial'});
test.beforeAll(async({request})=>{
  temporary=await mkdtemp(join(tmpdir(),'peritus-session-e2e-'));root=join(temporary,'project');await mkdir(root);
  await writeFile(join(root,'README.md'),'# Session integration fixture\n');
  await writeFile(join(temporary,'cli'),'#!/bin/sh\nprintf "%s\\n" "$@"\n',{mode:0o700});
  await startServer();
  const boot=await (await request.get(`${origin}/api/bootstrap`)).json();token=boot.token;project=boot.workspace.projects[0].id;session=boot.workspace.sessions[0].id;
});
test.afterAll(async()=>{await stopServer();if(temporary)await rm(temporary,{recursive:true,force:true});});

test('model choices survive reload and restart and remain session-local',async({page})=>{
  await page.goto('/');
  const composer=page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});
  await composer.fill('/model');await composer.press('Enter');
  await page.getByLabel('writer model',{exact:true}).fill('fixture-model');
  await page.locator('fieldset').first().locator('select').last().selectOption('high');
  await page.getByRole('button',{name:'Save choices',exact:true}).click();await expect(page.getByRole('dialog')).toBeHidden();
  await page.reload();await composer.fill('/model');await composer.press('Enter');
  await expect(page.getByLabel('writer model',{exact:true})).toHaveValue('fixture-model');
  await expect(page.locator('fieldset').first().locator('select').last()).toHaveValue('high');
  await page.getByRole('button',{name:'Close panel',exact:true}).click();
  await page.getByRole('button',{name:'Add session',exact:true}).click();
  await expect(page.locator('.session-tab.current button[role="tab"]')).not.toHaveAttribute('id',`session-${session}`);
  await expect(page.locator('.session-tab')).toHaveCount(2);
  await composer.fill('/model');await composer.press('Enter');
  await expect(page.getByLabel('writer model',{exact:true})).toHaveValue('');
  await page.getByRole('button',{name:'Close panel',exact:true}).click();
  await stopServer();await startServer();
  const saved=JSON.parse(await readFile(join(temporary,'workspace.json'),'utf8'));
  expect(saved.sessions.find((item:{id:string})=>item.id===session).settings.models.writer).toEqual({id:'fixture-model',manual:false,effort:'high'});
});

test('retained terminal output can be reopened and terminated after browser reload',async({page,request})=>{
  await page.goto('/');const composer=page.getByRole('textbox',{name:'Message Peritus or enter a slash command'});
  await composer.fill('/cli --version');await composer.press('Enter');
  await expect(page.getByText('Console process exited.',{exact:false})).toBeVisible();
  await page.reload();await composer.fill('/consoles');await composer.press('Enter');
  await expect(page.locator('.console-tabs button')).toHaveCount(1);
  await expect(page.getByText('Console process exited.',{exact:false})).toBeVisible();
  await page.getByRole('button',{name:'Terminate console',exact:true}).click();await expect(page.getByRole('dialog')).toBeHidden();
  expect((await (await request.get('/api/bootstrap')).json()).consoles).toEqual([]);
});

test('workbench binds exact browser session, project and daemon without executing the prepared command',async({request})=>{
  const boot=await (await request.get('/api/bootstrap')).json();token=boot.token;
  const opened=await action(request,'workbench',{session,suggestion:'/context next'});
  expect(opened).toMatchObject({session,project,suggestion:'/context next'});
  let terminal:any;
  await expect.poll(async()=>{terminal=await (await request.get(`/api/terminal/${opened.id}`,{headers:{'x-peritus-token':token}})).json();return terminal.ended;}).toBe(true);
  expect(Buffer.from(terminal.data,'base64').toString().replaceAll('\r','').trim().split('\n')).toEqual(['--endpoint',join(temporary,'absent.sock'),'open',root,'--run',session]);
  expect((await (await request.get('/api/bootstrap')).json()).consoles).toContainEqual(expect.objectContaining({id:opened.id,session,suggestion:'/context next'}));
  expect((await action(request,'workbench',{session:'missing'})).error).toContain('Session');
  await action(request,'close-console',{id:opened.id});
});
