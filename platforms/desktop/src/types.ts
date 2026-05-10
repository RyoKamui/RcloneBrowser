export type Theme = "system" | "light" | "dark";
export type TransferStatus = "queued" | "running" | "completed" | "failed" | "cancelled";
export type TransferDirection = "upload" | "download" | "copy";
export type TransferOperation = "copy" | "move" | "sync";
export type IconSize = "small" | "medium" | "large";

export interface ExportOptions {
  oneFileSystem: boolean;
  minSize: string;
  minAge: string;
  maxAge: string;
  maxDepth: number;
  excludes: string[];
  extraArgs: string[];
}

export interface Settings {
  rclonePath: string;
  configPath: string | null;
  defaultDownloadDir: string | null;
  defaultUploadDir: string | null;
  defaultDownloadArgs: string[];
  defaultUploadArgs: string[];
  showHidden: boolean;
  showFolderIcons: boolean;
  showFileIcons: boolean;
  alternatingRows: boolean;
  iconSize: IconSize;
  confirmDelete: boolean;
  theme: Theme;
  advancedArgs: string[];
  streamCommand: string;
  mountArgs: string[];
  closeToTray: boolean;
  alwaysShowTray: boolean;
  notifyFinishedTransfers: boolean;
  checkAppUpdates: boolean;
  checkRcloneUpdates: boolean;
  useProxy: boolean;
  httpProxy: string;
  httpsProxy: string;
  noProxy: string;
  exportOptions: ExportOptions;
  dualPane: boolean;
  showTransferShelf: boolean;
  compactRows: boolean;
}

export interface RcloneStatus {
  available: boolean;
  version: string | null;
  error: string | null;
}

export interface RcloneRelease {
  version: string;
  released: string | null;
  downloadUrl: string | null;
}

export interface RcloneUpdateInfo {
  currentVersion: string;
  stable: RcloneRelease | null;
  beta: RcloneRelease | null;
  stableUpdateAvailable: boolean;
}

export interface Remote {
  name: string;
  type: string;
  description: string;
  isLocal: boolean;
  displayName: string;
}

export interface ConfigProvider {
  name: string;
  description: string;
  prefix: string;
  hide: boolean;
}

export interface ConfigExample {
  value: string;
  help: string;
}

export interface ConfigOption {
  name: string;
  help: string;
  defaultStr: string;
  valueStr: string;
  required: boolean;
  isPassword: boolean;
  exclusive: boolean;
  sensitive: boolean;
  optionType: string;
  examples: ConfigExample[];
}

export interface ConfigQuestion {
  state: string;
  option: ConfigOption | null;
  error: string;
  result: string;
}

export interface Entry {
  name: string;
  path: string;
  isDir: boolean;
  size: number | null;
  modTime: string | null;
  mimeType: string | null;
}

export interface TransferRequest {
  direction: TransferDirection;
  operation: TransferOperation;
  source: string;
  destination: string;
  isDirectory: boolean;
  extraArgs: string[];
  label: string | null;
}

export interface TransferSnapshot {
  id: string;
  direction: TransferDirection;
  operation: TransferOperation;
  label: string | null;
  source: string;
  destination: string;
  isDirectory: boolean;
  extraArgs: string[];
  status: TransferStatus;
  bytes: number;
  totalBytes: number | null;
  speed: number | null;
  etaSeconds: number | null;
  checks: number;
  totalChecks: number | null;
  filesTransferred: number;
  totalFiles: number | null;
  errors: number;
  elapsedSeconds: number | null;
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
  logTail: string[];
}

export interface ActivitySnapshot {
  id: string;
  kind: "mount" | "stream";
  source: string;
  destination: string;
  status: TransferStatus;
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
  logTail: string[];
}

export interface SavedTask {
  id: string;
  description: string;
  direction: TransferDirection;
  operation: TransferOperation;
  source: string;
  destination: string;
  isDirectory: boolean;
  syncDeleteMode: "during" | "after" | "before" | null;
  update: boolean;
  ignoreExisting: boolean;
  compareMode: "sizeAndModTime" | "checksum" | "ignoreSize" | "sizeOnly" | "checksumIgnoreSize";
  oneFileSystem: boolean;
  noUpdateModtime: boolean;
  transfers: number;
  checkers: number;
  bandwidth: string;
  minSize: string;
  minAge: string;
  maxAge: string;
  maxDepth: number;
  connectTimeoutSeconds: number;
  idleTimeoutSeconds: number;
  retries: number;
  lowLevelRetries: number;
  deleteExcluded: boolean;
  excludes: string[];
  extraArgs: string[];
  sharedWithMe: boolean;
}

export interface DirectorySummary {
  count: number;
  bytes: number;
}

export interface UpdateStatus {
  currentVersion: string;
  latestVersion: string;
  available: boolean;
  releaseUrl: string;
}

export interface Bootstrap {
  appVersion: string;
  settings: Settings;
  rclone: RcloneStatus;
  remotes: Remote[];
  transfers: TransferSnapshot[];
  activities: ActivitySnapshot[];
  tasks: SavedTask[];
  portable: boolean;
  dataDirectory: string;
  homeDirectory: string;
}
