import {afterAll,beforeEach,expect,test,vi} from 'vitest';
import type {Bootstrap,Preferences} from '../../src/lib/types';

const api=vi.hoisted(()=>({bootstrap:vi.fn()}));
vi.mock('../../src/lib/api',()=>api);

const values=new Map<string,string>();
vi.stubGlobal('sessionStorage',{
  get length(){return values.size;},
  clear(){values.clear();},
  getItem(key:string){return values.get(key)??null;},
  key(index:number){return [...values.keys()][index]??null;},
  removeItem(key:string){values.delete(key);},
  setItem(key:string,value:string){values.set(key,value);},
} satisfies Storage);

const {clonePreferences,defaults,reconcileConfiguration,refresh,settingsSnapshot,ui}=await import('../../src/lib/workspace.svelte');

function resetSettings(){
  const preferences=clonePreferences(defaults);
  ui.workspace={projects:[],sessions:[]};ui.preferences=clonePreferences(preferences);ui.savedPreferences=clonePreferences(preferences);
  ui.config='theme = "nixie"\n';ui.configPath='/tmp/webui.toml';ui.configError='';
  Object.assign(ui.settings,{view:'controls',source:ui.config,initialized:true,preferencesRevision:0,preferencesBaselineRevision:0,sourceRevision:0,sourceBaselineRevision:0,activeOperation:'',requestOperation:''});
  values.clear();
}

beforeEach(()=>{api.bootstrap.mockReset();resetSettings();});
afterAll(()=>vi.unstubAllGlobals());

test('refresh preserves preferences changed outside the Settings revision owner',async()=>{
  ui.preferences.theme='blueprint';
  const persisted=clonePreferences(defaults);persisted.theme='daylight';
  api.bootstrap.mockResolvedValue({
    workspace:{projects:[],sessions:[]},consoles:[],pendingOperations:[],preferences:persisted,
    config:'theme = "daylight"\n',configError:null,configPath:'/tmp/webui.toml',token:'token',
  } satisfies Bootstrap);

  await refresh();

  expect(ui.preferences.theme).toBe('blueprint');
  expect(ui.savedPreferences.theme).toBe('daylight');
  expect(ui.config).toBe('theme = "daylight"\n');
});

test('counterpart saves preserve dirty values created outside Settings',()=>{
  ui.settings.source='theme = "blueprint"\n# unsaved source\n';
  const preferences=clonePreferences(defaults);preferences.theme='daylight';
  const preferencesSnapshot=settingsSnapshot();
  reconcileConfiguration('preferences',{preferences},{preferences,config:'theme = "daylight"\n'},preferencesSnapshot);
  expect(ui.settings.source).toBe('theme = "blueprint"\n# unsaved source\n');

  resetSettings();ui.preferences.theme='blueprint';
  const sourceSnapshot=settingsSnapshot(),parsed=clonePreferences(defaults) as Preferences;
  reconcileConfiguration('config',{text:'theme = "nixie"\n'},parsed as unknown as Record<string,unknown>,sourceSnapshot);
  expect(ui.preferences.theme).toBe('blueprint');
});
