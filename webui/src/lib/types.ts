export interface Project { id: string; root: string; name: string; repository: string; closed: boolean }
export interface Session { id: string; conversation: string; run: string; project: string; parent: string | null; title: string; closed: boolean; settings: SessionSettings }
export interface SessionSettings { models:Record<string,ModelChoice>; providers:Record<string,string> }
export interface ConsoleSession { id:string;title:string;project:string;session:string|null;suggestion:string;ended?:boolean }
export interface Workspace { projects: Project[]; sessions: Session[] }
export interface Preferences {
  theme: 'nixie' | 'daylight' | 'blueprint'; density: 'comfortable' | 'compact';
  motion: boolean; sound: boolean; font_size: number; font_family: string; mono_family: string;
  explorer_width: number; controls_visible: boolean; explorer_visible: boolean;
  word_wrap: boolean; markdown_preview: boolean;
  shortcuts: Record<string, string>; aliases: Record<string, string>; tokens: Record<string, string>;
}
export interface Bootstrap { workspace: Workspace; consoles?:ConsoleSession[]; pendingOperations?:import('./operations.svelte').PendingOperation[]; preferences: Preferences | null; config: string; configError: string | null; configPath: string; token: string }
export interface Entry { name: string; path: string; directory: boolean; symlink: boolean; bytes: number }
export interface Directory { entries: Entry[]; total: number; next: number | null }
export interface GitChange { code: string; path: string; from: string | null }
export interface GitBranch { name: string; ref: string; remote: boolean; current: boolean; upstream: string }
export interface GitRemote { name: string; fetch: string[]; push: string[] }
export interface GitStatus { root: string; branch: string; changes: GitChange[]; remotes: string; branches: GitBranch[]; remoteDetails: GitRemote[] }
export interface Activity { id: string; kind: 'user' | 'assistant' | 'tool' | 'status' | 'error'; text: string; detail: string }
export interface RunLegalControls { stop:boolean;retry:boolean;accept:boolean;commit:boolean;export:boolean;discard:boolean;acknowledge:boolean }
export interface RunOperation { kind:string;state:string;identity:string;known:string;uncertainty:string;legalControls:RunLegalControls }
export interface Run { providers?:Record<string,string>; id: string; workspace: string; phase: string; task: string; status: string; diff: string; gates: string; review: string; summary: string; operation:RunOperation; deliverable: {root: string; paths: string[]; instructions: string; qualification: string;accepted?:boolean;commitRevision?:string;exportPath?:string;discarded?:boolean} | null }
export interface WorkbenchNotice { conversation:string; revision:string; queued:boolean; started:boolean; observation:string; action:string; detail:string }
export interface Conversation { models?:Record<string,ModelChoice>; mode?:Mode; run?: Run; received?: string; incorporated?: string; activities?: Activity[]; workbench?:WorkbenchNotice }
export interface Facts { providers: {id: string; kind: string; model: string}[]; workspace: { id: string; root: string; execution: string; trust: string } | null; endpoint: string; ready:boolean;reason:string }
export interface Attachment {id:string;session:string;project:string;path:string;bytes:number;digest:string;media:string}
export interface ModelChoice { id: string; manual: boolean; effort: string }
export type Mode = 'chat' | 'plan' | 'review' | 'build';
export interface FileTab { path: string; session: string; project: string }

export interface ImprovementEvaluation {conversation:string;run:string;target:string}
export interface ImprovementCandidate { id:string;proposal:string;dismissed:boolean;evaluation:ImprovementEvaluation|null;evidence:{run:string;digest:string;summary:string}[] }
export interface ImprovementInbox {workspace:string;candidates:ImprovementCandidate[]}
