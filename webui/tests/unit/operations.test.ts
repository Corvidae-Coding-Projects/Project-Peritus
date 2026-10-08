import {expect,test} from 'vitest';
import {claimOperation,recovery,retainOperation} from '../../src/lib/operations.svelte';

test('overlapping receipt checks give one reconciler ownership',async()=>{
  recovery.pending=[];
  retainOperation({operation:'configuration-a',command:'config'});
  let releaseSlow!:()=>void,releaseFast!:()=>void;
  const slow=new Promise<void>(done=>releaseSlow=done),fast=new Promise<void>(done=>releaseFast=done);
  const checkingSlow=slow.then(()=>claimOperation('configuration-a'));
  const checkingFast=fast.then(()=>claimOperation('configuration-a'));

  releaseFast();
  expect((await checkingFast)?.operation).toBe('configuration-a');
  retainOperation({operation:'configuration-b',command:'preferences'});
  releaseSlow();

  expect(await checkingSlow).toBeUndefined();
  expect(recovery.pending.map(item=>item.operation)).toEqual(['configuration-b']);
  claimOperation('configuration-b');
});
