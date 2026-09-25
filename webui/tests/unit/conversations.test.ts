import {describe,it,expect} from 'vitest';
import {latestConversation} from '../../src/lib/conversations';
import type {Conversation,Run} from '../../src/lib/types';
const run={id:'exact-run'} as Run;
describe('cross-client observations',()=>{
  it('does not roll back models after a delayed poll',()=>{
    const current:Conversation={run,models:{writer:{id:'new',manual:false,effort:'high'}},activities:[{id:'9007199254740993',kind:'status',text:'saved',detail:''}]};
    const delayed:Conversation={run,models:{},activities:[{id:'9007199254740992',kind:'status',text:'old',detail:''}]};
    expect(latestConversation(current,delayed)).toBe(current);
    expect(latestConversation(current,{...delayed,activities:[{...delayed.activities![0]!,id:'9007199254740994'}]}).models).toEqual({});
  });
  it('keeps conversation settings and transcript when a run control returns only status',()=>{
    const current:Conversation={run,mode:'plan',models:{},activities:[]};
    expect(latestConversation(current,{run:{...run,status:'cancelled'}})).toEqual({...current,run:{...run,status:'cancelled'}});
  });
});
