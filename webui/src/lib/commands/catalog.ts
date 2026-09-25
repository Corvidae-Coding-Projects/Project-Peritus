export interface Command { id: string; label: string; detail: string; group: string; slash: string; args?: string }
export const commands: Command[] = [
  {id:'improvements',label:'Improvement inbox',detail:'Collect suggestions; explicitly generate and test selected harness patches',group:'Harness',slash:'/improvements'},
  {id:'new',label:'New conversation',detail:'Open a sibling session in this project',group:'Workspace',slash:'/new'},
  {id:'nest',label:'Nest a conversation',detail:'Add a child beneath the active session',group:'Workspace',slash:'/nest'},
  {id:'open',label:'Open project',detail:'Add another project to this browser tab',group:'Workspace',slash:'/open',args:'Absolute project directory'},
  {id:'files',label:'File explorer',detail:'Browse project files, including hidden and ignored files',group:'Workspace',slash:'/files'},
  {id:'git',label:'Source control',detail:'Branches, remotes, staging, commit, fetch, pull, and push',group:'Workspace',slash:'/git'},
  {id:'settings',label:'Configure console',detail:'Theme, behavior, layout, shortcuts, and dotfile',group:'Workspace',slash:'/settings'},
  {id:'sessions',label:'Session library',detail:'Reopen and organize saved sessions',group:'Workspace',slash:'/sessions'},
  {id:'chat',label:'Chat mode',detail:'Discuss or direct the harness',group:'Conversation',slash:'/chat'},
  {id:'plan',label:'Plan mode',detail:'Read-only planning and discussion',group:'Conversation',slash:'/plan'},
  {id:'review',label:'Review mode',detail:'Independent read-only review',group:'Conversation',slash:'/review'},
  {id:'build',label:'Build mode',detail:'Writer, checks, reviewer, and fixer pipeline',group:'Conversation',slash:'/build'},
  {id:'stop',label:'Stop current work',detail:'Keep effects already completed',group:'Conversation',slash:'/stop'},
  {id:'model',label:'Models and reasoning',detail:'Select models and effort for each role',group:'Conversation',slash:'/model'},
  {id:'effort',label:'Reasoning effort',detail:'Adjust per-role reasoning depth',group:'Conversation',slash:'/effort'},
  {id:'details',label:'Activity details',detail:'Inspect public tool and status observations',group:'Conversation',slash:'/details'},
  {id:'status',label:'Current run status',detail:'Inspect the observed daemon state',group:'Conversation',slash:'/status'},
  {id:'diff',label:'Candidate diff',detail:'Review exact candidate changes',group:'Results',slash:'/diff'},
  {id:'runs',label:'Run history',detail:'Inspect and resume daemon-owned runs',group:'Results',slash:'/runs'},
  {id:'retry',label:'Retry current run',detail:'Resume a failed or stopped run',group:'Results',slash:'/retry'},
  {id:'export',label:'Export candidate',detail:'Export the exact candidate patch',group:'Results',slash:'/export'},
  {id:'accept',label:'Accept candidate',detail:'Open the exact-run CLI acceptance flow',group:'Results',slash:'/accept'},
  {id:'commit',label:'Commit candidate',detail:'Commit the managed candidate; distinct from Git commit',group:'Results',slash:'/commit'},
  {id:'discard',label:'Discard candidate',detail:'Remove this run’s candidate changes',group:'Results',slash:'/discard'},
  {id:'consoles',label:'Retained consoles',detail:'Reopen or terminate this project’s CLI processes',group:'Harness',slash:'/consoles'},
  {id:'terminal',label:'Full harness console',detail:'Interactive Peritus CLI with keyboard and mouse controls',group:'Harness',slash:'/terminal'},
  {id:'providers',label:'Provider setup',detail:'Account login, API routes, models, and capability tests',group:'Harness',slash:'/providers'},
  {id:'workspaces',label:'Workspace setup',detail:'Register, trust, repair, or forget a workspace',group:'Harness',slash:'/workspaces'},
  {id:'reconnect',label:'Reconnect',detail:'Reload workspace and inspect original operation receipts',group:'Harness',slash:'/reconnect'},
  {id:'update',label:'Update Peritus',detail:'Run the interactive product update',group:'Harness',slash:'/update'},
  ...[
    ['approvals','Approvals','Inspect and answer pending approvals',''],['doctor','Diagnostics','Inspect local prerequisites',''],
    ['trace','Execution trace','Inspect the current execution trace',''],['queue','Input queue','Add, hold, release, edit, or withdraw queued input','add <message>'],
    ['context','Model context','Inspect eligible input and sealed context manifests','next'],['compact','Compact context','Preview and confirm a local compaction',''],
    ['brief','Project brief','Inspect or update objective, acceptance, and constraints','objective <text>'],['goal','Persistent goal','Define criteria and a bounded goal','<objective>'],
    ['pause','Pause goal','Pause at a selected safe boundary','after-operation'],['resume','Resume goal','Resume a paused persistent goal',''],
    ['usage','Usage accounting','Inspect measured requests, tools, time, and tokens',''],['budget','Execution budget','Set time, request, tool, and token limits','time=30m'],
    ['preview','Result preview','Launch, capture, check, and annotate results','results'],['checkpoint','Checkpoint','Create or inspect a durable checkpoint','<name>'],
    ['rewind','Rewind','Preview exact restore effects before confirmation','<checkpoint>'],['attach','Attach context','Explicitly attach a file or supported image','<path>'],
    ['permissions','Workspace permissions','Inspect and restrict or grant capabilities',''],['init','Project initialization','Discover local controls and preview instruction files',''],
    ['memory','Working memory','Save, revise, scope, pin, and forget guidance',''],['fork','Fork conversation','Create a read-only or isolated branch','read-only'],
    ['run','Execute candidate','Run the candidate’s recorded instructions',''],
  ].map(([id,label,detail,args]) => ({ id:id!,label:label!,detail:detail!,args:args!,group:'Workbench console',slash:`/${id}` })),
  {id:'cli',label:'Scriptable CLI command',detail:'Artifacts, prompts, events, terminal attachments, shutdown, and completions',group:'Harness',slash:'/cli',args:'status'},
  {id:'help',label:'Command directory',detail:'Every function, with a mouse action and slash command',group:'Workspace',slash:'/help'},
  {id:'close-project',label:'Close project',detail:'Close this project tab and retain its sessions',group:'Workspace',slash:'/close-project'},
];
export function matchesShortcut(event: KeyboardEvent, binding: string): boolean {
  const parts = binding.toLowerCase().split('+'); const key = parts.pop();
  return event.key.toLowerCase() === key && (event.metaKey || event.ctrlKey) === parts.includes('mod') && event.shiftKey === parts.includes('shift') && event.altKey === parts.includes('alt');
}
export function formatShortcut(binding: string | undefined, apple: boolean): string {
  const names: Record<string,string> = {mod:apple?'Cmd':'Ctrl',alt:apple?'Option':'Alt',shift:'Shift'};
  return binding?.split('+').map(part=>names[part.toLowerCase()]??(part.length===1?part.toUpperCase():part)).join(' ')??'';
}
