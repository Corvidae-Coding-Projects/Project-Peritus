export interface Project { id: string; root: string; name: string; repository: string; closed: boolean }
export interface Session { id: string; conversation: string; run: string; project: string; parent: string | null; title: string; closed: boolean; settings: SessionSettings }
export interface SessionSettings { models:Record<string,ModelChoice>; providers:Record<string,string> }
export interface ConsoleSession { id:string;workspace:string;title:string;project:string;session:string|null;suggestion:string;ended?:boolean;available?:boolean;recovery?:string }
export interface Workspace { projects: Project[]; sessions: Session[] }
export interface Preferences {
  theme: 'nixie' | 'daylight' | 'blueprint'; density: 'comfortable' | 'compact';
  motion: boolean; sound: boolean; font_size: number; font_family: string; mono_family: string;
  explorer_width: number; controls_visible: boolean; explorer_visible: boolean;
  word_wrap: boolean; markdown_preview: boolean;
  shortcuts: Record<string, string>; aliases: Record<string, string>; tokens: Record<string, string>;
}
export interface Bootstrap { identity: string; workspace: Workspace; consoles?:ConsoleSession[]; pendingOperations?:import('./operations.svelte').PendingOperation[]; pendingOperationsCursor?:string|null;pendingOperationsSnapshot?:string; preferences: Preferences | null; config: string; configError: string | null; configPath: string; token: string }
export interface Entry { name: string; path: string; directory: boolean; directoryIdentity: string | null; symlink: boolean; bytes: number }
export interface Directory { ready:boolean;phase:'pending'|'indexing'|'ordering'|'completed';cursor:string|null;directory:string;filter:string;offset:number;snapshot:string;entries:Entry[];indexed:number;total:number|null;next:string|null }
export interface GitChange { code: string; path: string; from: string | null }
export interface GitBranch { name: string; ref: string; remote: boolean; current: boolean; upstream: string }
export interface GitRemote { name: string; fetch: string[]; push: string[] }
export interface GitStatus { root: string; branch: string; changes: GitChange[]; remotes: string; branches: GitBranch[]; remoteDetails: GitRemote[]; inventoryCursor:string|null; inventorySnapshot:string }
export interface GitInventoryPage { branches:GitBranch[];remoteDetails:GitRemote[];cursor:string|null;snapshot:string }
export interface GitOutputPage { stream:'stdout'|'stderr';offset:string;previous:string|null;next:string|null;bytes:string;digest:string;pageBytes:string;pageDigest:string;data:string;error?:string|null }
export interface GitEffectResult { operation:string;status:number|null;stdout:GitOutputPage;stderr:GitOutputPage }
export interface Activity { id: string; kind: 'user' | 'assistant' | 'tool' | 'status' | 'error'; text: string; detail: string }
export interface RunLegalControls { stop:boolean;retry:boolean;accept:boolean;commit:boolean;export:boolean;discard:boolean;acknowledge:boolean }
export interface RunOperation { kind:string;state:string;identity:string;known:string;uncertainty:string;legalControls:RunLegalControls }
export interface Run { providers?:Record<string,string>; id: string; workspace: string; phase: string; task: string; status: string; diff: string; gates: string; review: string; summary: string; operation:RunOperation; deliverable: {root: string; paths: string[]; instructions: string; qualification: string;accepted?:boolean;commitRevision?:string;exportPath?:string;discarded?:boolean} | null }
export interface RunPage {runs:Run[];cursor:string|null;store:string}
export interface WorkbenchNotice { conversation:string; revision:string; queued:boolean; started:boolean; observation:string; action:string; detail:string }
export interface Conversation { models?:Record<string,ModelChoice>; mode?:Mode; run?: Run; received?: string; incorporated?: string; activities?: Activity[]; workbench?:WorkbenchNotice }
export interface Facts { providers: {id: string; kind: string; model: string}[]; workspace: { id: string; root: string; execution: string; trust: string } | null; endpoint: string; ready:boolean;reason:string }
export interface Attachment {id:string;session:string;project:string;path:string;bytes:number;digest:string;media:string}
export interface ModelChoice { id: string; manual: boolean; effort: string }
export type Mode = 'chat' | 'plan' | 'review' | 'build';
export interface FileTab { path: string; session: string; project: string }

export interface ImprovementEvaluation {conversation:string;run:string;target:string}
export interface ImprovementTextReference {digest:string;bytes:string;source:string}
export interface ImprovementCandidate {id:string;proposal:ImprovementTextReference;evidenceCount:string;dismissed:boolean;evaluation:ImprovementEvaluation|null}
export interface ImprovementPage {workspace:string;revision:string;candidates:ImprovementCandidate[];next:string|null}
export interface ImprovementEvidence {run:string;summary:ImprovementTextReference}
export interface ImprovementEvidencePage {workspace:string;candidate:string;revision:string;evidence:ImprovementEvidence[];next:string|null}
export interface ImprovementTextPage {offset:string;text:string;next:string|null}
