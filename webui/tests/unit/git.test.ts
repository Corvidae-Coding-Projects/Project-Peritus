import { describe, expect, it } from 'vitest';
import { gitCommand } from '../../src/lib/commands/git';

describe('Git command routing', () => {
  it('keeps status a read-only refresh', () => {
    expect(gitCommand(['status'])).toEqual({kind:'status'});
    expect(gitCommand(['status','--short'])).toEqual({kind:'status'});
    expect(()=>gitCommand(['status','--unexpected'])).toThrow();
  });
  it('passes exact branch names and start points without shell expansion', () => {
    expect(gitCommand(['branch','create','feature/docs','origin/develop'])).toEqual({kind:'branch-create',options:{branch:'feature/docs',start:'origin/develop'}});
    expect(gitCommand(['switch','feature/docs'])).toEqual({kind:'branch-switch',options:{branch:'feature/docs'}});
  });
  it('requires confirmation for removing branches and remotes', () => {
    for(const args of [['branch','delete','feature/docs'],['remote','remove','origin']]) {
      expect(gitCommand(args)).toMatchObject({options:{confirmed:true},confirmation:expect.any(String)});
    }
  });
  it('keeps explicit upstream publishing separate from ordinary push', () => {
    expect(gitCommand(['push','origin','feature/docs','--set-upstream'])).toEqual({kind:'push',options:{remote:'origin',branch:'feature/docs',setUpstream:true}});
    expect(gitCommand(['push'])).toEqual({kind:'push',options:{remote:'',branch:'',setUpstream:false}});
  });
});
