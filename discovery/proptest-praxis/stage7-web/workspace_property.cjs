#!/usr/bin/env node
/* Deterministic actual-module scheduler for the Stage 7 browser workspace. */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const crypto = require('node:crypto');

const HERE = __dirname;
const ROOT = path.resolve(HERE, '../../..');
const OUT = path.join(ROOT, 'target/stage7-web-ts');
const TSC = path.join(ROOT, 'webui/node_modules/typescript/bin/tsc');
const compiler = spawnSync(process.execPath, [TSC,
  '--module', 'commonjs', '--target', 'es2022', '--moduleResolution', 'node',
  '--skipLibCheck', '--outDir', OUT, '--rootDir', path.join(ROOT, 'webui/src'),
  path.join(HERE, 'runes.d.ts'), path.join(ROOT, 'webui/src/lib/workspace.svelte.ts')],
{cwd: ROOT, encoding: 'utf8'});
if (compiler.status !== 0) {
  process.stderr.write(compiler.stdout + compiler.stderr);
  process.exit(2);
}

global.$state = value => value;
const workspace = require(path.join(OUT, 'lib/workspace.svelte.js'));
const operations = require(path.join(OUT, 'lib/operations.svelte.js'));
const conversations = require(path.join(OUT, 'lib/conversations.js'));

function response(value, status = 200) {
  return {status, ok: status >= 200 && status < 300, async json() { return value; }};
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {resolve = yes; reject = no;});
  return {promise, resolve, reject};
}
async function until(predicate) {
  for (let i = 0; i < 20; i++) {
    if (predicate()) return;
    await new Promise(resolve => setImmediate(resolve));
  }
  throw new Error('deterministic request was not issued');
}
function storage() {
  const values = new Map();
  return {getItem: key => values.get(key) ?? null,
    setItem: (key, value) => {values.set(key, String(value));},
    removeItem: key => {values.delete(key);},
    key: index => [...values.keys()][index] ?? null,
    get length() {return values.size;}};
}
global.sessionStorage = storage();
global.localStorage = storage();
global.confirm = () => true;

function reset(original) {
  const {ui} = workspace;
  ui.workspace = {projects:[{id:'p1',closed:false,name:'One',root:'/tmp/p1',repository:'/tmp/p1'},
    {id:'p2',closed:false,name:'Two',root:'/tmp/p2',repository:'/tmp/p2'}],
    sessions:[{id:'s1',project:'p1',closed:false,title:'Existing',settings:{models:{},providers:{}}},
      {id:'s2',project:'p2',closed:false,title:'Existing',settings:{models:{},providers:{}}}]};
  ui.projectId = 'p1'; ui.sessionId = 's1'; ui.ready = true;
  ui.facts = {ready:true, providers:[], workspace:{id:'w1',root:'/tmp/p1',execution:'',trust:''}, endpoint:'', reason:''};
  ui.drafts = {'s1':original}; ui.pending = {}; ui.attaching = {};
  ui.attachments = {}; ui.conversations = {}; ui.modes = {};
  ui.notice = ''; ui.noticeError = false;
  operations.recovery.pending = [];
  global.sessionStorage = storage();
}

async function caseRun(original, edited, outcome, switchProject = false) {
  reset(original);
  const waiting = deferred();
  let actionSeen = false, actionCount = 0, queryCount = 0, sent;
  global.fetch = async (url, options) => {
    if (url === '/api/action') {
      sent = JSON.parse(options.body);
      actionSeen = true;
      actionCount++;
      return waiting.promise;
    }
    if (url.startsWith('/api/query?')) {
      const query = new URL(url, 'http://local.test').searchParams;
      assert.equal(query.get('kind'), 'operation');
      queryCount++;
      if (queryCount === 1) return response(null);
      return response({input:{text:sent.text,attachments:[]},
        result:{activities:[{id:'9007199254740993',kind:'user',text:sent.text,detail:''}],received:'9007199254740993'}});
    }
    throw new Error(`unexpected fetch ${url}`);
  };
  const pending = workspace.send();
  await until(() => actionSeen);
  workspace.ui.drafts.s1 = edited;
  if (switchProject) {workspace.ui.projectId='p2';workspace.ui.sessionId='s2';workspace.ui.drafts.s2='untouched';}
  if (outcome === 'direct') waiting.resolve(response({activities:[{id:'9007199254740993',kind:'user',text:sent.text,detail:''}]}));
  else waiting.reject(new Error('synthetic connection loss'));
  if (outcome === 'direct') await pending;
  else {
    await assert.rejects(pending, /Outcome uncertain/);
    assert.equal(operations.recovery.pending.length, 1);
    if (outcome === 'recovered-reload' || outcome === 'recovered-no-client-metadata') {
      if (outcome === 'recovered-no-client-metadata') global.sessionStorage.removeItem(`peritus:operation:${sent.operation}`);
      operations.recovery.pending = [];
      operations.restoreOperations([{operation:sent.operation,command:'send',session:'s1',project:'p1'}]);
    }
    const result = await workspace.reconcileOperations();
    assert.deepEqual(result, {recovered:1,unresolved:0,failed:0});
  }
  assert.equal(sent.text, original.trim());
  assert.equal(actionCount, 1, 'uncertain mutation must not be resent');
  if (switchProject) {
    assert.equal(workspace.ui.projectId, 'p2'); assert.equal(workspace.ui.sessionId, 's2');
    assert.equal(workspace.ui.drafts.s2, 'untouched');
  }
  return {remaining:workspace.ui.drafts.s1, pending:operations.recovery.pending.length};
}

function generator(seed, count) {
  let state = seed >>> 0;
  const next = () => {state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0;};
  const texts = ['a','Shared https://doc.qt.io for reading.','é','a😊b','𐐷','hello'];
  const spaces = ['', ' ', '  ', '\t', '\n'];
  const output = [];
  for (let i = 0; i < count; i++) {
    const content = texts[next() % texts.length];
    const original = spaces[next() % spaces.length] + content + spaces[next() % spaces.length];
    let edited;
    if (next() % 4 === 0) edited = content + '!';
    else edited = spaces[next() % spaces.length] + content + spaces[next() % spaces.length];
    if (edited === original) edited = ' ' + edited;
    output.push({original, edited, generated:true});
  }
  return output;
}
function source() {
  const files = ['api.ts','workspace.svelte.ts','operations.svelte.ts','conversations.ts'];
  return Object.fromEntries(files.map(file => [file,
    crypto.createHash('sha256').update(fs.readFileSync(path.join(ROOT,'webui/src/lib',file))).digest('hex')]));
}
async function inspect(testCase) {
  const direct = await caseRun(testCase.original, testCase.edited, 'direct', !!testCase.switchProject);
  const recovered = await caseRun(testCase.original, testCase.edited, 'recovered', !!testCase.switchProject);
  const reloaded = await caseRun(testCase.original, testCase.edited, 'recovered-reload', !!testCase.switchProject);
  const noMetadata = await caseRun(testCase.original, testCase.edited, 'recovered-no-client-metadata', !!testCase.switchProject);
  const expected = testCase.edited === testCase.original ? '' : testCase.edited;
  return {case:testCase, expected, direct, recovered, reloaded, noMetadata,
    valid:direct.remaining === expected && recovered.remaining === expected &&
      reloaded.remaining === expected && noMetadata.remaining === testCase.edited};
}
async function projectResponseCase() {
  reset('');
  const replies = new Map();
  global.fetch = async url => {
    const parsed = new URL(url, 'http://local.test');
    const key = `${parsed.searchParams.get('kind')}:${parsed.searchParams.get('project')}`;
    const wait = deferred(); replies.set(key,wait);
    return wait.promise;
  };
  const first = workspace.selectProject('p1');
  await until(() => replies.size===2);
  const second = workspace.selectProject('p2');
  await until(() => replies.size===4);
  const facts = id => ({ready:true,workspace:{id,root:`/tmp/${id}`},providers:[],endpoint:'',reason:''});
  const git = id => ({root:`/tmp/${id}`,branch:id,changes:[],remotes:'',branches:[],remoteDetails:[]});
  replies.get('facts:p2').resolve(response(facts('p2')));
  replies.get('git:p2').resolve(response(git('p2')));
  await second;
  replies.get('facts:p1').resolve(response(facts('p1')));
  replies.get('git:p1').resolve(response(git('p1')));
  await first;
  assert.equal(workspace.ui.projectId,'p2');
  assert.equal(workspace.ui.sessionId,'s2');
  assert.equal(workspace.ui.facts.workspace.id,'p2','late facts from old project cannot replace current facts');
  assert.equal(workspace.ui.git.branch,'p2','late git from old project cannot replace current git');
}
function conversationOrderCases() {
  const make = (run,id,tag) => ({run:run?{id:run}:undefined,activities:[{id:String(id),kind:'user',text:tag,detail:''}]});
  let current = make('r1',9007199254740993n,'first');
  let count = 0;
  for(let i=0;i<512;i++) {
    const earlier=make('r1',9007199254740992n+BigInt(i),'older');
    const later=make('r1',9007199254740994n+BigInt(i),'later');
    const expected=BigInt(earlier.activities[0].id)<BigInt(current.activities[0].id)?current:earlier;
    assert.deepEqual(conversations.latestConversation(current,earlier),expected);
    current=conversations.latestConversation(current,later);
    assert.equal(current.activities[0].text,'later');
    count++;
  }
  const nextRun=make('r2',1n,'new run');
  assert.deepEqual(conversations.latestConversation(current,nextRun),nextRun,'new run resets sequence comparison');
  return count+1;
}
async function main() {
  const outputIndex = process.argv.indexOf('--output');
  const output = outputIndex < 0 ? null : path.resolve(process.argv[outputIndex+1]);
  const cases = [
    {original:'Shared https://doc.qt.io for reading.', edited:' Shared https://doc.qt.io for reading. ', fixed:true},
    {original:'a', edited:' a', fixed:true},
    {original:'  hello  ', edited:'hello ', fixed:true},
    {original:'é', edited:' é ', fixed:true},
    {original:' a ', edited:' a ', fixed:true},
    {original:'a', edited:' a', switchProject:true, fixed:true},
    ...generator(7319, 512),
  ];
  let first, completed = 0;
  for (const candidate of cases) {
    const observed = await inspect(candidate);
    completed++;
    if (!observed.valid) {first = observed;break;}
  }
  if(!first){await projectResponseCase();conversationOrderCases();}
  let minimized;
  if (first) {
    minimized = await inspect({original:'a',edited:' a',fixed:true});
    if (minimized.valid) minimized = first;
  }
  const report = {status:first?'fail':'pass',seed:7319,requested_cases:cases.length,
    completed_cases:completed,source:source(),failure:first?.case??null,
    metrics:{fixed:Math.min(completed,6),generated:Math.max(0,completed-6),
      recovered_draft_loss:first&&first.recovered.remaining!==first.expected?1:0,
      stale_project_schedule:first?0:1,decimal_u64_conversation_sequences:first?0:513}};
  if (output) {
    fs.mkdirSync(output,{recursive:false});
    fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');
    if (first) {
      fs.writeFileSync(path.join(output,'first-failure.json'),JSON.stringify(first,null,2)+'\n');
      fs.writeFileSync(path.join(output,'minimized-failure.json'),JSON.stringify(minimized,null,2)+'\n');
    }
  }
  console.log(JSON.stringify(report));
  if (first) process.exitCode = 1;
}
main().catch(error => {console.error(error);process.exitCode = 1;});
