import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import "./styles.css";
import type {
  ActivitySnapshot,
  Bootstrap,
  ConfigProvider,
  ConfigQuestion,
  DirectorySummary,
  Entry,
  ExportOptions,
  Remote,
  RcloneUpdateInfo,
  SavedTask,
  Settings,
  TransferRequest,
  TransferSnapshot,
  UpdateStatus,
} from "./types";

type Page = "browser" | "transfers" | "tasks" | "settings";
type SortKey = "name" | "size" | "modified";
type SettingsTab = "general" | "connection" | "transfers" | "advanced";

interface BrowserTab {
  id: string;
  remote: string;
  path: string;
  sharedWithMe: boolean;
  history: string[];
  historyIndex: number;
}

type PaneId = "primary" | "secondary";

interface BrowserPane {
  entries: Entry[];
  tabs: BrowserTab[];
  currentTabId: string | null;
  selectedPath: string | null;
  browserError: string | null;
  loading: boolean;
  search: string;
  sort: SortKey;
  sortAscending: boolean;
  menuOpen: boolean;
  cache: Map<string, Entry[]>;
}

function createBrowserPane(): BrowserPane {
  return {
    entries: [], tabs: [], currentTabId: null, selectedPath: null,
    browserError: null, loading: true, search: "", sort: "name",
    sortAscending: true, menuOpen: false, cache: new Map(),
  };
}

interface SimpleDialog {
  kind: "simple";
  title: string;
  message?: string;
  label?: string;
  value?: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: (value: string) => Promise<void>;
}

interface InfoDialog {
  kind: "info";
  title: string;
  message: string;
  details?: string;
}

interface RcloneUpdateDialog {
  kind: "rclone-update";
  info: RcloneUpdateInfo;
}

interface TaskDialog {
  kind: "task";
  title: string;
  task: SavedTask;
}

interface ExportDialog {
  kind: "export";
  format: "txt" | "csv";
  destination: string;
  options: ExportOptions;
}

interface LocationsDialog {
  kind: "locations";
  providers: ConfigProvider[];
  search: string;
  name: string;
  provider: string;
  question: ConfigQuestion | null;
  started: boolean;
  busy: boolean;
}

type DialogState = SimpleDialog | InfoDialog | RcloneUpdateDialog | TaskDialog | ExportDialog | LocationsDialog;

interface Toast {
  id: number;
  message: string;
  kind: "success" | "error";
}

interface AppState {
  page: Page;
  settingsTab: SettingsTab;
  appVersion: string;
  settings: Settings | null;
  rclone: Bootstrap["rclone"] | null;
  remotes: Remote[];
  transfers: TransferSnapshot[];
  activities: ActivitySnapshot[];
  tasks: SavedTask[];
  panes: Record<PaneId, BrowserPane>;
  activePane: PaneId;
  entries: Entry[];
  tabs: BrowserTab[];
  currentTabId: string | null;
  selectedPath: string | null;
  browserError: string | null;
  loading: boolean;
  search: string;
  sort: SortKey;
  sortAscending: boolean;
  menuOpen: boolean;
  cache: Map<string, Entry[]>;
  dialog: DialogState | null;
  toasts: Toast[];
  dragActive: boolean;
  portable: boolean;
  dataDirectory: string;
}

const appRoot = document.querySelector<HTMLDivElement>("#app");
if (!appRoot) throw new Error("Application root was not found");
const root = appRoot;

let paneContext: PaneId | null = null;

const state: AppState = {
  page: "browser",
  settingsTab: "general",
  appVersion: "",
  settings: null,
  rclone: null,
  remotes: [],
  transfers: [],
  activities: [],
  tasks: [],
  panes: { primary: createBrowserPane(), secondary: createBrowserPane() },
  activePane: "primary",
  get entries() { return browserPane().entries; },
  set entries(value) { browserPane().entries = value; },
  get tabs() { return browserPane().tabs; },
  set tabs(value) { browserPane().tabs = value; },
  get currentTabId() { return browserPane().currentTabId; },
  set currentTabId(value) { browserPane().currentTabId = value; },
  get selectedPath() { return browserPane().selectedPath; },
  set selectedPath(value) { browserPane().selectedPath = value; },
  get browserError() { return browserPane().browserError; },
  set browserError(value) { browserPane().browserError = value; },
  get loading() { return browserPane().loading; },
  set loading(value) { browserPane().loading = value; },
  get search() { return browserPane().search; },
  set search(value) { browserPane().search = value; },
  get sort() { return browserPane().sort; },
  set sort(value) { browserPane().sort = value; },
  get sortAscending() { return browserPane().sortAscending; },
  set sortAscending(value) { browserPane().sortAscending = value; },
  get menuOpen() { return browserPane().menuOpen; },
  set menuOpen(value) { browserPane().menuOpen = value; },
  get cache() { return browserPane().cache; },
  set cache(value) { browserPane().cache = value; },
  dialog: null,
  toasts: [],
  dragActive: false,
  portable: false,
  dataDirectory: "",
};

function browserPane(id: PaneId = paneContext ?? state.activePane): BrowserPane {
  return state.panes[id];
}

function withPane<T>(id: PaneId, operation: () => T): T {
  const previous = paneContext;
  paneContext = id;
  try { return operation(); }
  finally { paneContext = previous; }
}

let toastSequence = 0;

root.addEventListener("click", (event) => void handleClick(event));
root.addEventListener("dblclick", (event) => void handleDoubleClick(event));
root.addEventListener("contextmenu", handleContextMenu);
root.addEventListener("input", handleInput);
root.addEventListener("keydown", (event) => void handleKeyDown(event));

render();
void initialize();

async function initialize(): Promise<void> {
  try {
    await listen<TransferSnapshot>("transfer:update", ({ payload }) => {
      const previous = state.transfers.find((transfer) => transfer.id === payload.id);
      upsertById(state.transfers, payload);
      if (previous?.status === "running" && !isActive(payload) && state.settings?.notifyFinishedTransfers) {
        void notifyTransferFinished(payload);
      }
      renderTransferBadge();
      if (state.page === "transfers") renderMain();
    });
    await listen<ActivitySnapshot>("activity:update", ({ payload }) => {
      upsertById(state.activities, payload);
      renderTransferBadge();
      if (state.page === "transfers") renderMain();
    });
    await listen("app:quit-requested", () => void confirmQuit());
    await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") {
        state.dragActive = true;
        renderDragOverlay();
      } else if (payload.type === "leave") {
        state.dragActive = false;
        renderDragOverlay();
      } else if (payload.type === "drop") {
        state.dragActive = false;
        renderDragOverlay();
        void uploadPaths(payload.paths);
      }
    });
    const data = await invoke<Bootstrap>("bootstrap");
    applyBootstrap(data);
    const firstRemote = data.remotes.find((remote) => !remote.isLocal) ?? data.remotes[0];
    const localRemote = data.remotes.find((remote) => remote.isLocal) ?? data.remotes[0];
    if (localRemote) withPane("primary", () => {
      openRemoteTab(localRemote.name);
      const tab = currentTab();
      if (tab) {
        tab.path = data.homeDirectory;
        tab.history = [data.homeDirectory];
      }
    });
    if (firstRemote) withPane("secondary", () => openRemoteTab(firstRemote.name));
    if (localRemote || firstRemote) {
      render();
      await Promise.all([
        localRemote ? loadEntries(false, "primary") : Promise.resolve(),
        firstRemote ? loadEntries(false, "secondary") : Promise.resolve(),
      ]);
    }
    if (state.settings?.notifyFinishedTransfers) void ensureNotificationPermission();
    void automaticUpdateChecks();
  } catch (error) {
    state.loading = false;
    state.rclone = { available: false, version: null, error: errorMessage(error) };
  }
  render();
}

function applyBootstrap(data: Bootstrap): void {
  state.appVersion = data.appVersion;
  state.settings = data.settings;
  state.rclone = data.rclone;
  state.remotes = data.remotes;
  state.transfers = data.transfers;
  state.activities = data.activities;
  state.tasks = data.tasks;
  state.portable = data.portable;
  state.dataDirectory = data.dataDirectory;
  applyTheme();
  applyAppearance();
}

async function handleClick(event: MouseEvent): Promise<void> {
  const paneChanged = activatePaneFrom(event.target);
  const target = (event.target as HTMLElement).closest<HTMLElement>("[data-action]");
  if (target?.classList.contains("modal-backdrop") && (event.target as HTMLElement).closest(".modal")) return;
  if (!target) {
    if (state.menuOpen) {
      state.menuOpen = false;
      renderMain();
    } else if (paneChanged) renderMain();
    return;
  }
  const action = target.dataset.action;
  if (!action || target.matches(":disabled")) return;

  try {
    switch (action) {
      case "nav-browser": state.page = "browser"; render(); break;
      case "nav-transfers": state.page = "transfers"; render(); break;
      case "nav-tasks": state.page = "tasks"; render(); break;
      case "nav-settings": state.page = "settings"; render(); break;
      case "select-settings-tab": selectSettingsTab((target.dataset.tab as SettingsTab) ?? "general"); break;
      case "select-remote":
        openRemoteTab(target.dataset.remote ?? "");
        state.page = "browser";
        await loadEntries();
        render();
        break;
      case "select-tab":
        selectTab(target.dataset.id ?? "");
        await loadEntries(false);
        render();
        break;
      case "new-tab": addBrowserTab(); await loadEntries(false); render(); break;
      case "close-tab":
        event.stopPropagation();
        await closeTab(target.dataset.id ?? "");
        break;
      case "open-path": await navigate(target.dataset.path ?? ""); break;
      case "nav-back": await navigateHistory(-1); break;
      case "nav-forward": await navigateHistory(1); break;
      case "nav-up": await navigate(parentBrowserPath(currentTab()?.path ?? "")); break;
      case "open-entry": await openEntry(target.dataset.path ?? ""); break;
      case "select-entry":
        state.selectedPath = target.dataset.path ?? null;
        state.menuOpen = false;
        renderMain();
        break;
      case "refresh": await loadEntries(true); renderMain(); break;
      case "refresh-remotes": await refreshConnection(); break;
      case "manage-locations": await showLocationsDialog(); break;
      case "select-provider": selectConfigProvider(target.dataset.provider ?? ""); break;
      case "start-location-config": await startLocationConfig(); break;
      case "continue-location-config": await continueLocationConfig(); break;
      case "cancel-location-config": await cancelLocationConfig(); break;
      case "location-config-terminal":
        await invoke("open_rclone_config");
        state.dialog = null;
        renderModal();
        showToast("Full rclone setup opened in Terminal. Reload Locations when it is finished.", "success");
        break;
      case "sort": setSort((target.dataset.sort as SortKey) ?? "name"); renderMain(); break;
      case "toggle-menu": state.menuOpen = !state.menuOpen; renderMain(); break;
      case "toggle-shared":
        if (currentTab()) {
          currentTab()!.sharedWithMe = (target as HTMLInputElement).checked;
          state.cache.clear();
          await loadEntries(true);
          render();
        }
        break;
      case "new-folder": showNewFolderDialog(); break;
      case "rename": showRenameDialog(); break;
      case "move": showMoveDialog(); break;
      case "delete": showDeleteDialog(); break;
      case "public-link": await copyPublicLink(); break;
      case "copy-path": await copySelectedPath(); break;
      case "upload": await upload(false); break;
      case "upload-folder": await upload(true); break;
      case "download": await downloadSelected(); break;
      case "copy-other-pane": await transferToOtherPane("copy"); break;
      case "move-other-pane": await transferToOtherPane("move"); break;
      case "toggle-dual-pane": await toggleDualPane(); break;
      case "advanced-transfer": showTransferDialog(); break;
      case "get-size": await showSize(); break;
      case "get-tree": await showTree(); break;
      case "export-txt": showExportDialog("txt"); break;
      case "export-csv": showExportDialog("csv"); break;
      case "choose-export-destination": await chooseExportDestination(); break;
      case "export-confirm": await runExport(); break;
      case "export-reset":
        if (state.dialog?.kind === "export") {
          state.dialog.options = defaultExportOptions();
          renderModal();
        }
        break;
      case "mount": await mountSelected(); break;
      case "stream": await streamSelected(); break;
      case "cancel-transfer":
        if (await ask("Cancel this running transfer?", { title: "Rclone Browser", kind: "warning" })) {
          await invoke("cancel_transfer", { id: target.dataset.id });
        }
        break;
      case "cancel-activity":
        if (await ask("Stop this mount or stream?", { title: "Rclone Browser", kind: "warning" })) {
          await invoke("cancel_activity", { id: target.dataset.id });
        }
        break;
      case "copy-transfer-command": await copyTransferCommand(target.dataset.id ?? ""); break;
      case "clear-transfers":
        await invoke("clear_finished_transfers");
        state.transfers = state.transfers.filter(isActive);
        state.activities = state.activities.filter(isActive);
        render();
        break;
      case "new-task": showTaskDialog(defaultTask()); break;
      case "edit-task": {
        const task = state.tasks.find((item) => item.id === target.dataset.id);
        if (task) showTaskDialog(structuredClone(task));
        break;
      }
      case "run-task": await runTask(target.dataset.id ?? "", false); break;
      case "dry-task": await runTask(target.dataset.id ?? "", true); break;
      case "delete-task": await confirmDeleteTask(target.dataset.id ?? ""); break;
      case "copy-task-command": await copyTaskCommand(target.dataset.id ?? ""); break;
      case "task-save": await saveTaskFromDialog(); break;
      case "task-run": await runTaskFromDialog(false); break;
      case "task-dry": await runTaskFromDialog(true); break;
      case "task-reset": resetTaskOptions(); break;
      case "choose-task-source": await chooseTaskPath("source"); break;
      case "choose-task-destination": await chooseTaskPath("destination"); break;
      case "save-settings": await saveSettings(); break;
      case "test-rclone": await saveSettings(false); await refreshConnection(); break;
      case "open-config": await invoke("open_rclone_config"); showToast("rclone configuration opened in Terminal", "success"); break;
      case "reconnect": await reconnectCurrent(); break;
      case "check-rclone-update": await checkRcloneUpdate(); break;
      case "download-rclone":
        await invoke("open_rclone_download", {
          channel: target.dataset.channel ?? "",
          version: target.dataset.version ?? "",
        });
        break;
      case "check-app-update": await checkAppUpdate(); break;
      case "choose-rclone": await choosePath("rclone-path", false); break;
      case "choose-config": await choosePath("config-path", false); break;
      case "choose-downloads": await choosePath("download-path", true); break;
      case "choose-uploads": await choosePath("upload-path", true); break;
      case "close-modal": state.dialog = null; renderModal(); break;
      case "confirm-modal": await confirmSimpleDialog(); break;
    }
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

async function handleDoubleClick(event: MouseEvent): Promise<void> {
  activatePaneFrom(event.target);
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-entry-row]");
  if (!row) return;
  await openEntry(row.dataset.path ?? "");
}

function handleContextMenu(event: MouseEvent): void {
  activatePaneFrom(event.target);
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-entry-row]");
  if (!row) return;
  event.preventDefault();
  state.selectedPath = row.dataset.path ?? null;
  state.menuOpen = true;
  renderMain();
}

function handleInput(event: Event): void {
  activatePaneFrom(event.target);
  const input = event.target as HTMLInputElement;
  if (input.matches("[data-file-search]")) {
    state.search = input.value;
    renderTable();
  } else if (input.id === "location-search" && state.dialog?.kind === "locations") {
    state.dialog.search = input.value;
    const list = document.querySelector<HTMLElement>("#provider-list");
    if (list) list.innerHTML = configProviderListMarkup(state.dialog);
    const count = document.querySelector<HTMLElement>("#provider-count");
    if (count) count.textContent = configProviderCount(state.dialog);
  } else if (input.getAttribute("name") === "theme") {
    document.documentElement.dataset.theme = input.value;
  }
}

async function handleKeyDown(event: KeyboardEvent): Promise<void> {
  activatePaneFrom(event.target);
  const settingsTab = (event.target as HTMLElement).closest<HTMLElement>("[data-settings-tab]");
  if (settingsTab && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
    event.preventDefault();
    const tabs: SettingsTab[] = ["general", "connection", "transfers", "advanced"];
    const current = tabs.indexOf((settingsTab.dataset.tab as SettingsTab) ?? "general");
    const offset = event.key === "ArrowRight" ? 1 : -1;
    const next = tabs[(current + offset + tabs.length) % tabs.length];
    selectSettingsTab(next);
    document.querySelector<HTMLElement>(`[data-settings-tab][data-tab="${next}"]`)?.focus();
    return;
  }
  if (event.key === "Escape") {
    if (state.dialog) {
      if (state.dialog.kind === "locations") await cancelLocationConfig();
      else {
        state.dialog = null;
        renderModal();
      }
    } else if (state.menuOpen) {
      state.menuOpen = false;
      renderMain();
    }
  }
  if (event.key === "Enter" && state.dialog?.kind === "simple") await confirmSimpleDialog();
  if (event.key === "Enter" && state.dialog?.kind === "locations") {
    event.preventDefault();
    if (state.dialog.question) await continueLocationConfig();
    else await startLocationConfig();
  }
  if (event.key === "Enter" && !state.dialog) {
    const row = (event.target as HTMLElement).closest<HTMLElement>("[data-entry-row]");
    if (row) {
      event.preventDefault();
      await openEntry(row.dataset.path ?? "");
    }
  }
}

function activatePaneFrom(target: EventTarget | null): boolean {
  const value = (target as HTMLElement | null)?.closest<HTMLElement>("[data-pane]")?.dataset.pane;
  if ((value === "primary" || value === "secondary") && state.activePane !== value) {
    state.activePane = value;
    return true;
  }
  return false;
}

function openRemoteTab(remote: string): void {
  if (!remote) return;
  let tab = state.tabs.find((candidate) => candidate.remote === remote);
  if (!tab) {
    tab = { id: crypto.randomUUID(), remote, path: "", sharedWithMe: false, history: [""], historyIndex: 0 };
    state.tabs.push(tab);
  }
  state.currentTabId = tab.id;
  state.search = "";
  state.selectedPath = null;
}

function addBrowserTab(): void {
  const current = currentTab();
  const remote = current?.remote ?? state.remotes[0]?.name;
  if (!remote) return;
  const tab: BrowserTab = {
    id: crypto.randomUUID(), remote, path: current?.path ?? "", sharedWithMe: current?.sharedWithMe ?? false,
    history: [current?.path ?? ""], historyIndex: 0,
  };
  state.tabs.push(tab);
  state.currentTabId = tab.id;
  state.selectedPath = null;
  state.search = "";
}

function selectTab(id: string): void {
  if (state.tabs.some((tab) => tab.id === id)) {
    state.currentTabId = id;
    state.search = "";
    state.selectedPath = null;
  }
}

async function closeTab(id: string): Promise<void> {
  const index = state.tabs.findIndex((tab) => tab.id === id);
  if (index < 0) return;
  state.tabs.splice(index, 1);
  if (state.currentTabId === id) {
    const replacement = state.tabs[Math.max(0, index - 1)] ?? null;
    state.currentTabId = replacement?.id ?? null;
    state.entries = [];
    if (replacement) await loadEntries(false);
  }
  render();
}

function currentTab(id: PaneId = paneContext ?? state.activePane): BrowserTab | undefined {
  const pane = browserPane(id);
  return pane.tabs.find((tab) => tab.id === pane.currentTabId);
}

function currentRemote(id: PaneId = paneContext ?? state.activePane): Remote | undefined {
  const tab = currentTab(id);
  return state.remotes.find((remote) => remote.name === tab?.remote);
}

async function navigate(path: string, recordHistory = true): Promise<void> {
  const tab = currentTab();
  if (!tab) return;
  if (recordHistory && path !== tab.path) {
    tab.history = tab.history.slice(0, tab.historyIndex + 1);
    tab.history.push(path);
    tab.historyIndex = tab.history.length - 1;
  }
  tab.path = path;
  state.selectedPath = null;
  state.search = "";
  await loadEntries(false);
  renderMain();
}

async function navigateHistory(offset: -1 | 1): Promise<void> {
  const tab = currentTab();
  if (!tab) return;
  const nextIndex = tab.historyIndex + offset;
  if (nextIndex < 0 || nextIndex >= tab.history.length) return;
  tab.historyIndex = nextIndex;
  await navigate(tab.history[nextIndex], false);
}

async function openEntry(path: string): Promise<void> {
  const entry = state.entries.find((candidate) => candidate.path === path);
  if (entry?.isDir) await navigate(entry.path);
}

function parentBrowserPath(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  if (!trimmed) return "";
  const separator = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (separator <= 0) return "";
  return trimmed.slice(0, separator);
}

function cacheKey(tab: BrowserTab): string {
  return `${tab.remote}\u0000${tab.sharedWithMe}\u0000${tab.path}`;
}

async function loadEntries(force = false, paneId: PaneId = state.activePane): Promise<void> {
  const pane = browserPane(paneId);
  const tab = currentTab(paneId);
  if (!tab) return;
  const key = cacheKey(tab);
  const cached = pane.cache.get(key);
  if (cached && !force) {
    pane.entries = cached;
    pane.browserError = null;
    pane.loading = false;
    return;
  }
  pane.loading = true;
  pane.browserError = null;
  pane.selectedPath = null;
  renderMain();
  try {
    const entries = await invoke<Entry[]>("list_entries", {
      remote: tab.remote,
      path: tab.path,
      sharedWithMe: tab.sharedWithMe,
    });
    pane.entries = entries;
    pane.cache.set(key, entries);
  } catch (error) {
    pane.entries = [];
    pane.browserError = errorMessage(error);
  } finally {
    pane.loading = false;
  }
}

async function refreshConnection(): Promise<void> {
  state.loading = true;
  render();
  const data = await invoke<Bootstrap>("bootstrap");
  applyBootstrap(data);
  const fallback = data.remotes.find((remote) => remote.isLocal) ?? data.remotes[0];
  for (const id of ["primary", "secondary"] as const) {
    const pane = browserPane(id);
    pane.cache.clear();
    pane.tabs = pane.tabs.filter((tab) => data.remotes.some((remote) => remote.name === tab.remote));
    if (!pane.tabs.some((tab) => tab.id === pane.currentTabId)) pane.currentTabId = pane.tabs[0]?.id ?? null;
    if (!pane.currentTabId && fallback) withPane(id, () => openRemoteTab(fallback.name));
  }
  await Promise.all((["primary", "secondary"] as const).map((id) => currentTab(id) ? loadEntries(true, id) : Promise.resolve()));
  showToast(data.rclone.available ? "rclone is connected" : "Could not connect to rclone", data.rclone.available ? "success" : "error");
  render();
}

async function showLocationsDialog(): Promise<void> {
  const dialog: LocationsDialog = {
    kind: "locations",
    providers: [],
    search: "",
    name: "",
    provider: "",
    question: null,
    started: false,
    busy: true,
  };
  state.dialog = dialog;
  renderModal();
  try {
    dialog.providers = await invoke<ConfigProvider[]>("list_config_providers");
    dialog.busy = false;
    if (state.dialog === dialog) renderModal();
  } catch (error) {
    if (state.dialog === dialog) {
      state.dialog = null;
      renderModal();
    }
    throw error;
  }
}

function syncLocationForm(dialog: LocationsDialog): void {
  const form = document.querySelector<HTMLFormElement>("#location-form");
  if (!form) return;
  const data = new FormData(form);
  dialog.name = String(data.get("name") ?? dialog.name).trimStart();
  dialog.search = String(data.get("search") ?? dialog.search);
}

function selectConfigProvider(provider: string): void {
  if (state.dialog?.kind !== "locations" || state.dialog.question) return;
  syncLocationForm(state.dialog);
  if (!state.dialog.providers.some((candidate) => candidate.name === provider)) return;
  state.dialog.provider = provider;
  renderModal();
  window.requestAnimationFrame(() => document.querySelector<HTMLInputElement>("#location-name")?.focus());
}

async function startLocationConfig(): Promise<void> {
  if (state.dialog?.kind !== "locations" || state.dialog.question || state.dialog.busy) return;
  const dialog = state.dialog;
  syncLocationForm(dialog);
  dialog.name = dialog.name.trim();
  if (!dialog.name) {
    showToast("Enter a name for the new location.", "error");
    document.querySelector<HTMLInputElement>("#location-name")?.focus();
    return;
  }
  if (!dialog.provider) {
    showToast("Choose one of the rclone storage protocols.", "error");
    return;
  }
  dialog.busy = true;
  renderModal();
  try {
    const question = await invoke<ConfigQuestion>("start_location_config", {
      name: dialog.name,
      provider: dialog.provider,
    });
    dialog.started = Boolean(question.state || question.option);
    if (state.dialog !== dialog) {
      if (dialog.started) await invoke("cancel_location_config", { name: dialog.name });
      return;
    }
    if (!question.state && !question.option) {
      await finishLocationConfig(dialog);
      return;
    }
    dialog.question = question;
    dialog.busy = false;
    renderModal();
    focusConfigAnswer();
  } catch (error) {
    dialog.busy = false;
    if (state.dialog === dialog) renderModal();
    throw error;
  }
}

async function cancelLocationConfig(): Promise<void> {
  if (state.dialog?.kind !== "locations" || state.dialog.busy) return;
  const dialog = state.dialog;
  if (!dialog.started) {
    state.dialog = null;
    renderModal();
    return;
  }
  dialog.busy = true;
  renderModal();
  try {
    await invoke("cancel_location_config", { name: dialog.name });
    if (state.dialog === dialog) {
      state.dialog = null;
      renderModal();
      showToast("Location setup cancelled; no placeholder was kept.", "success");
    }
  } catch (error) {
    dialog.busy = false;
    if (state.dialog === dialog) renderModal();
    throw error;
  }
}

async function continueLocationConfig(): Promise<void> {
  if (state.dialog?.kind !== "locations" || !state.dialog.question || state.dialog.busy) return;
  const dialog = state.dialog;
  const currentQuestion = dialog.question!;
  const option = currentQuestion.option;
  const field = document.querySelector<HTMLInputElement | HTMLSelectElement>("#location-answer");
  if (!option || !field) return;
  const result = field.value;
  if (option.required && !result.trim()) {
    showToast(`${friendlyConfigName(option.name)} is required.`, "error");
    field.focus();
    return;
  }
  dialog.busy = true;
  renderModal();
  try {
    const question = await invoke<ConfigQuestion>("continue_location_config", {
      name: dialog.name,
      provider: dialog.provider,
      sessionState: currentQuestion.state,
      result,
    });
    if (state.dialog !== dialog) return;
    if (!question.state && !question.option) {
      await finishLocationConfig(dialog);
      return;
    }
    dialog.question = question;
    dialog.busy = false;
    renderModal();
    focusConfigAnswer();
  } catch (error) {
    dialog.busy = false;
    if (state.dialog === dialog) renderModal();
    throw error;
  }
}

async function finishLocationConfig(dialog: LocationsDialog): Promise<void> {
  const name = dialog.name;
  state.dialog = null;
  renderModal();
  await refreshConnection();
  if (state.remotes.some((remote) => remote.name === name)) {
    openRemoteTab(name);
    state.page = "browser";
    await loadEntries(true);
    render();
  }
  showToast(`Location “${name}” was added.`, "success");
}

function focusConfigAnswer(): void {
  window.requestAnimationFrame(() => document.querySelector<HTMLInputElement | HTMLSelectElement>("#location-answer")?.focus());
}

function showNewFolderDialog(): void {
  const tab = currentTab();
  if (!tab) return;
  state.dialog = {
    kind: "simple",
    title: "New folder",
    label: "Folder name",
    value: "",
    confirmLabel: "Create folder",
    onConfirm: async (name) => {
      await invoke("create_folder", { remote: tab.remote, parent: tab.path, name });
      invalidateCurrent();
      await loadEntries(true);
      showToast(`Created “${name}”`, "success");
      renderMain();
    },
  };
  renderModal();
  focusModalInput();
}

function showRenameDialog(): void {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || !tab) return;
  state.menuOpen = false;
  state.dialog = {
    kind: "simple",
    title: "Rename item",
    label: "New name",
    value: entry.name,
    confirmLabel: "Rename",
    onConfirm: async (newName) => {
      await invoke("rename_entry", { remote: tab.remote, path: entry.path, newName });
      invalidateCurrent();
      await loadEntries(true);
      showToast(`Renamed to “${newName}”`, "success");
      renderMain();
    },
  };
  renderModal();
  focusModalInput(true);
}

function showMoveDialog(): void {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || !tab) return;
  state.menuOpen = false;
  state.dialog = {
    kind: "simple",
    title: `Move “${entry.name}”`,
    message: "Enter its new path on this remote, including the item name.",
    label: "Destination path",
    value: entry.path,
    confirmLabel: "Move",
    onConfirm: async (destination) => {
      await invoke("move_entry", { remote: tab.remote, source: entry.path, destination });
      state.cache.clear();
      await loadEntries(true);
      showToast(`Moved “${entry.name}”`, "success");
      renderMain();
    },
  };
  renderModal();
  focusModalInput(true);
}

function showDeleteDialog(): void {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || !tab) return;
  const performDelete = async (): Promise<void> => {
    await invoke("delete_entry", { remote: tab.remote, path: entry.path, isDir: entry.isDir });
    invalidateCurrent();
    await loadEntries(true);
    showToast(`Deleted “${entry.name}”`, "success");
    renderMain();
  };
  if (!state.settings?.confirmDelete) {
    void performDelete().catch((error) => showToast(errorMessage(error), "error"));
    return;
  }
  state.menuOpen = false;
  state.dialog = {
    kind: "simple",
    title: `Delete “${entry.name}”?`,
    message: entry.isDir ? "This removes the folder and everything inside it. This action cannot be undone." : "This action cannot be undone.",
    confirmLabel: "Delete",
    danger: true,
    onConfirm: performDelete,
  };
  renderModal();
}

async function confirmSimpleDialog(): Promise<void> {
  const dialog = state.dialog;
  if (!dialog || dialog.kind !== "simple") return;
  const value = document.querySelector<HTMLInputElement>("#modal-input")?.value.trim() ?? "";
  if (dialog.label && !value) return;
  const button = document.querySelector<HTMLButtonElement>("[data-action='confirm-modal']");
  button?.setAttribute("disabled", "");
  try {
    await dialog.onConfirm(value);
    state.dialog = null;
    renderModal();
  } catch (error) {
    button?.removeAttribute("disabled");
    showToast(errorMessage(error), "error");
  }
}

async function upload(directory: boolean): Promise<void> {
  const result = await open({
    directory,
    multiple: !directory,
    defaultPath: state.settings?.defaultUploadDir ?? undefined,
  });
  if (!result) return;
  await uploadPaths(Array.isArray(result) ? result : [result]);
}

async function uploadPaths(localPaths: string[]): Promise<void> {
  const tab = currentTab();
  if (!tab || !localPaths.length || state.page !== "browser") return;
  if (tab.sharedWithMe) {
    showToast("Google Shared with me is read-only in the browser", "error");
    return;
  }
  await invoke<string[]>("start_upload", {
    remote: tab.remote,
    path: tab.path,
    localPaths,
    sharedWithMe: tab.sharedWithMe,
    extraArgs: [],
  });
  showToast(`${localPaths.length} upload${localPaths.length === 1 ? "" : "s"} started`, "success");
  state.page = "transfers";
  render();
}

async function downloadSelected(): Promise<void> {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || !tab) return;
  const destination = await open({ directory: true, multiple: false, defaultPath: state.settings?.defaultDownloadDir ?? undefined });
  if (!destination || Array.isArray(destination)) return;
  await invoke<string>("start_download", {
    remote: tab.remote,
    entry,
    destinationDirectory: destination,
    sharedWithMe: tab.sharedWithMe,
    extraArgs: [],
  });
  showToast(`Downloading “${entry.name}”`, "success");
  state.page = "transfers";
  render();
}

async function transferToOtherPane(operation: "copy" | "move"): Promise<void> {
  const sourcePaneId = state.activePane;
  const destinationPaneId: PaneId = sourcePaneId === "primary" ? "secondary" : "primary";
  const sourceTab = currentTab(sourcePaneId);
  const destinationTab = currentTab(destinationPaneId);
  const entry = browserPane(sourcePaneId).entries.find((candidate) => candidate.path === browserPane(sourcePaneId).selectedPath);
  if (!sourceTab || !destinationTab || !entry) return;
  const destinationPath = joinBrowserPath(destinationTab.remote, destinationTab.path, entry.name);
  const request: TransferRequest = {
    direction: "copy",
    operation,
    source: browserEndpoint(sourceTab.remote, entry.path),
    destination: browserEndpoint(destinationTab.remote, destinationPath),
    isDirectory: entry.isDir,
    extraArgs: sourceTab.sharedWithMe ? ["--drive-shared-with-me"] : [],
    label: `${capitalize(operation)} ${entry.name}`,
  };
  await invoke("start_custom_transfer", { request });
  showToast(`${capitalize(operation)} started`, "success");
  render();
}

async function toggleDualPane(): Promise<void> {
  if (!state.settings) return;
  state.settings.dualPane = !state.settings.dualPane;
  if (!state.settings.dualPane) state.activePane = "primary";
  await invoke("save_settings", { settings: state.settings });
  render();
}

function browserEndpoint(remote: string, path: string): string {
  return remote === "__local__" ? path : `${remote.replace(/:+$/, "")}:${path.replace(/^\/+/, "")}`;
}

function joinBrowserPath(remote: string, parent: string, name: string): string {
  if (remote !== "__local__") return [parent.replace(/\/+$/, ""), name.replace(/^\/+/, "")].filter(Boolean).join("/");
  const separator = parent.includes("\\") ? "\\" : "/";
  const base = parent.replace(/[\\/]+$/, "") || separator;
  return base === separator ? `${base}${name}` : `${base}${separator}${name}`;
}

async function copyPublicLink(): Promise<void> {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || !tab) return;
  const link = await invoke<string>("get_public_link", { remote: tab.remote, path: entry.path, sharedWithMe: tab.sharedWithMe });
  await navigator.clipboard.writeText(link);
  state.menuOpen = false;
  showToast("Public link copied", "success");
  renderMain();
}

async function copySelectedPath(): Promise<void> {
  const tab = currentTab();
  const entry = selectedEntry();
  if (!tab || !entry) return;
  const value = tab.remote === "__local__" ? entry.path : `${tab.remote}:${entry.path}`;
  await navigator.clipboard.writeText(value);
  showToast("rclone path copied", "success");
}

function selectedFolderTarget(): { tab: BrowserTab; path: string } | null {
  const tab = currentTab();
  if (!tab) return null;
  const entry = selectedEntry();
  return { tab, path: entry?.isDir ? entry.path : tab.path };
}

async function showSize(): Promise<void> {
  const target = selectedFolderTarget();
  if (!target) return;
  const summary = await invoke<DirectorySummary>("get_directory_size", {
    remote: target.tab.remote, path: target.path, sharedWithMe: target.tab.sharedWithMe,
  });
  state.dialog = { kind: "info", title: "Folder size", message: `${summary.count.toLocaleString()} objects · ${formatSize(summary.bytes)}` };
  renderModal();
}

async function showTree(): Promise<void> {
  const target = selectedFolderTarget();
  if (!target) return;
  const tree = await invoke<string>("get_directory_tree", {
    remote: target.tab.remote, path: target.path, sharedWithMe: target.tab.sharedWithMe,
  });
  state.dialog = { kind: "info", title: "Directory tree", message: target.path || currentRemote()?.displayName || "Root", details: tree };
  renderModal();
}

function showExportDialog(format: "txt" | "csv"): void {
  const target = selectedFolderTarget();
  if (!target) return;
  state.menuOpen = false;
  state.dialog = {
    kind: "export",
    format,
    destination: `${lastPathPart(target.path) || target.tab.remote}-listing.${format}`,
    options: structuredClone(state.settings?.exportOptions ?? defaultExportOptions()),
  };
  renderModal();
}

async function chooseExportDestination(): Promise<void> {
  const dialog = state.dialog;
  if (!dialog || dialog.kind !== "export") return;
  const current = document.querySelector<HTMLInputElement>("#export-destination")?.value.trim();
  const destination = await save({
    defaultPath: current || dialog.destination,
    filters: [{ name: dialog.format === "csv" ? "CSV file" : "Text file", extensions: [dialog.format] }],
  });
  if (!destination) return;
  dialog.destination = destination;
  renderModal();
}

async function runExport(): Promise<void> {
  const dialog = state.dialog;
  const target = selectedFolderTarget();
  const form = document.querySelector<HTMLFormElement>("#export-form");
  if (!dialog || dialog.kind !== "export" || !target || !form) return;
  const data = new FormData(form);
  const destination = String(data.get("destination") ?? "").trim();
  if (!destination) throw new Error("Choose a destination file first.");
  const options: ExportOptions = {
    oneFileSystem: data.get("oneFileSystem") === "on",
    minSize: String(data.get("minSize") ?? "").trim(),
    minAge: String(data.get("minAge") ?? "").trim(),
    maxAge: String(data.get("maxAge") ?? "").trim(),
    maxDepth: Math.max(0, Number(data.get("maxDepth")) || 0),
    excludes: lines(String(data.get("excludes") ?? "")),
    extraArgs: lines(String(data.get("extraArgs") ?? "")),
  };
  const count = await invoke<number>("export_listing", {
    remote: target.tab.remote,
    path: target.path,
    sharedWithMe: target.tab.sharedWithMe,
    destination,
    format: dialog.format,
    options,
  });
  if (state.settings) {
    state.settings.exportOptions = options;
    await invoke("save_settings", { settings: state.settings });
  }
  state.dialog = null;
  renderModal();
  showToast(`Exported ${count.toLocaleString()} files`, "success");
}

function defaultExportOptions(): ExportOptions {
  return { oneFileSystem: false, minSize: "", minAge: "", maxAge: "", maxDepth: 0, excludes: [], extraArgs: [] };
}

async function mountSelected(): Promise<void> {
  const target = selectedFolderTarget();
  if (!target) return;
  const destination = await open({ directory: true, multiple: false });
  if (!destination || Array.isArray(destination)) return;
  await invoke("start_mount", {
    remote: target.tab.remote, path: target.path, destination, sharedWithMe: target.tab.sharedWithMe, extraArgs: [],
  });
  showToast("Mount started", "success");
  state.page = "transfers";
  render();
}

async function streamSelected(): Promise<void> {
  const entry = selectedEntry();
  const tab = currentTab();
  if (!entry || entry.isDir || !tab) return;
  await invoke("start_stream", { remote: tab.remote, path: entry.path, playerCommand: null, sharedWithMe: tab.sharedWithMe });
  showToast(`Streaming “${entry.name}”`, "success");
  state.page = "transfers";
  render();
}

function showTransferDialog(): void {
  const tab = currentTab();
  if (!tab) return;
  const entry = selectedEntry();
  const remotePath = tab.remote === "__local__"
    ? (entry?.path ?? tab.path)
    : `${tab.remote}:${entry?.path ?? tab.path}`;
  const task = defaultTask();
  task.description = entry ? `Transfer ${entry.name}` : `Transfer ${currentRemote()?.displayName ?? tab.remote}`;
  task.source = remotePath;
  task.destination = state.settings?.defaultDownloadDir ?? "";
  task.direction = "download";
  task.isDirectory = entry?.isDir ?? true;
  task.sharedWithMe = tab.sharedWithMe;
  task.extraArgs = [...(state.settings?.defaultDownloadArgs ?? [])];
  showTaskDialog(task, "Advanced transfer");
}

function defaultTask(): SavedTask {
  return {
    id: crypto.randomUUID(), description: "", direction: "copy", operation: "copy", source: "", destination: "",
    isDirectory: true, syncDeleteMode: null, update: false, ignoreExisting: false, compareMode: "sizeAndModTime",
    oneFileSystem: false, noUpdateModtime: false, transfers: 4, checkers: 8, bandwidth: "", minSize: "",
    minAge: "", maxAge: "", maxDepth: 0, connectTimeoutSeconds: 60, idleTimeoutSeconds: 300,
    retries: 3, lowLevelRetries: 10, deleteExcluded: false, excludes: [], extraArgs: [], sharedWithMe: false,
  };
}

function showTaskDialog(task: SavedTask, title = task.description ? "Edit task" : "New task"): void {
  state.dialog = { kind: "task", title, task };
  renderModal();
}

function resetTaskOptions(): void {
  const dialog = state.dialog;
  if (!dialog || dialog.kind !== "task") return;
  const current = readTaskForm();
  const defaults = defaultTask();
  dialog.task = {
    ...defaults,
    id: current.id,
    description: current.description,
    direction: current.direction,
    operation: current.operation,
    source: current.source,
    destination: current.destination,
    isDirectory: current.isDirectory,
    sharedWithMe: current.sharedWithMe,
    extraArgs: [...(current.direction === "upload" ? state.settings?.defaultUploadArgs ?? [] : current.direction === "download" ? state.settings?.defaultDownloadArgs ?? [] : [])],
  };
  renderModal();
}

function readTaskForm(): SavedTask {
  const dialog = state.dialog;
  if (!dialog || dialog.kind !== "task") throw new Error("Task editor is no longer open");
  const form = document.querySelector<HTMLFormElement>("#task-form");
  if (!form) throw new Error("Task editor was not found");
  const data = new FormData(form);
  const number = (name: string, fallback: number) => Number(data.get(name)) || fallback;
  return {
    ...dialog.task,
    description: String(data.get("description") ?? "").trim(),
    direction: String(data.get("direction") ?? "copy") as SavedTask["direction"],
    operation: String(data.get("operation") ?? "copy") as SavedTask["operation"],
    source: String(data.get("source") ?? "").trim(),
    destination: String(data.get("destination") ?? "").trim(),
    isDirectory: data.get("isDirectory") === "on",
    syncDeleteMode: nullable(String(data.get("syncDeleteMode") ?? "")) as SavedTask["syncDeleteMode"],
    update: data.get("update") === "on",
    ignoreExisting: data.get("ignoreExisting") === "on",
    compareMode: String(data.get("compareMode") ?? "sizeAndModTime") as SavedTask["compareMode"],
    oneFileSystem: data.get("oneFileSystem") === "on",
    noUpdateModtime: data.get("noUpdateModtime") === "on",
    transfers: number("transfers", 4),
    checkers: number("checkers", 8),
    bandwidth: String(data.get("bandwidth") ?? "").trim(),
    minSize: String(data.get("minSize") ?? "").trim(),
    minAge: String(data.get("minAge") ?? "").trim(),
    maxAge: String(data.get("maxAge") ?? "").trim(),
    maxDepth: Number(data.get("maxDepth")) || 0,
    connectTimeoutSeconds: number("connectTimeoutSeconds", 60),
    idleTimeoutSeconds: number("idleTimeoutSeconds", 300),
    retries: number("retries", 3),
    lowLevelRetries: number("lowLevelRetries", 10),
    deleteExcluded: data.get("deleteExcluded") === "on",
    excludes: lines(String(data.get("excludes") ?? "")),
    extraArgs: lines(String(data.get("extraArgs") ?? "")),
    sharedWithMe: data.get("sharedWithMe") === "on",
  };
}

async function chooseTaskPath(field: "source" | "destination"): Promise<void> {
  const form = document.querySelector<HTMLFormElement>("#task-form");
  if (!form) return;
  const directory = field === "destination" || form.querySelector<HTMLInputElement>("[name='isDirectory']")?.checked === true;
  const input = form.querySelector<HTMLInputElement>(`[name='${field}']`);
  const result = await open({ directory, multiple: false, defaultPath: input?.value || undefined });
  if (result && !Array.isArray(result) && input) input.value = result;
}

async function saveTaskFromDialog(): Promise<void> {
  const task = readTaskForm();
  const saved = await invoke<SavedTask>("save_task", { task });
  upsertById(state.tasks, saved);
  state.dialog = null;
  render();
  showToast("Task saved", "success");
}

async function runTaskFromDialog(dryRun: boolean): Promise<void> {
  const task = readTaskForm();
  const extraArgs = buildTaskArgs(task, dryRun);
  const request: TransferRequest = {
    direction: task.direction,
    operation: task.operation,
    source: task.source,
    destination: task.destination,
    isDirectory: task.isDirectory,
    extraArgs,
    label: task.description || null,
  };
  await invoke("start_custom_transfer", { request });
  state.dialog = null;
  state.page = "transfers";
  render();
  showToast(dryRun ? "Dry run started" : "Transfer started", "success");
}

function buildTaskArgs(task: SavedTask, dryRun: boolean): string[] {
  const args: string[] = [];
  if (dryRun) args.push("--dry-run");
  if (task.operation === "sync" && task.syncDeleteMode) args.push(`--delete-${task.syncDeleteMode}`);
  if (task.update) args.push("--update");
  if (task.ignoreExisting) args.push("--ignore-existing");
  if (task.compareMode === "checksum" || task.compareMode === "checksumIgnoreSize") args.push("--checksum");
  if (task.compareMode === "ignoreSize" || task.compareMode === "checksumIgnoreSize") args.push("--ignore-size");
  if (task.compareMode === "sizeOnly") args.push("--size-only");
  if (task.oneFileSystem) args.push("--one-file-system");
  if (task.noUpdateModtime) args.push("--no-update-modtime");
  args.push("--transfers", String(task.transfers), "--checkers", String(task.checkers));
  pushArg(args, "--bwlimit", task.bandwidth);
  pushArg(args, "--min-size", task.minSize);
  pushArg(args, "--min-age", task.minAge);
  pushArg(args, "--max-age", task.maxAge);
  if (task.maxDepth) args.push("--max-depth", String(task.maxDepth));
  args.push("--contimeout", `${task.connectTimeoutSeconds}s`, "--timeout", `${task.idleTimeoutSeconds}s`);
  args.push("--retries", String(task.retries), "--low-level-retries", String(task.lowLevelRetries));
  if (task.deleteExcluded) args.push("--delete-excluded");
  task.excludes.forEach((exclude) => args.push("--exclude", exclude));
  args.push(...task.extraArgs);
  if (task.sharedWithMe) args.push("--drive-shared-with-me");
  return args;
}

function pushArg(args: string[], name: string, value: string): void {
  if (value) args.push(name, value);
}

async function runTask(id: string, dryRun: boolean): Promise<void> {
  await invoke("run_task", { id, dryRun });
  state.page = "transfers";
  render();
  showToast(dryRun ? "Dry run started" : "Task started", "success");
}

async function confirmDeleteTask(id: string): Promise<void> {
  const task = state.tasks.find((item) => item.id === id);
  if (!task) return;
  if (!await ask(`Delete the saved task “${task.description}”?`, { title: "Rclone Browser", kind: "warning" })) return;
  await invoke("delete_task", { id });
  state.tasks = state.tasks.filter((item) => item.id !== id);
  renderMain();
}

async function copyTaskCommand(id: string): Promise<void> {
  const task = state.tasks.find((item) => item.id === id);
  if (!task) return;
  const command = await invoke<string>("copy_command", {
    operation: task.operation, source: task.source, destination: task.destination, isDirectory: task.isDirectory, extraArgs: buildTaskArgs(task, false),
  });
  await navigator.clipboard.writeText(command);
  showToast("Command copied", "success");
}

async function copyTransferCommand(id: string): Promise<void> {
  const transfer = state.transfers.find((item) => item.id === id);
  if (!transfer) return;
  const command = await invoke<string>("copy_command", {
    operation: transfer.operation,
    source: transfer.source,
    destination: transfer.destination,
    isDirectory: transfer.isDirectory,
    extraArgs: transfer.extraArgs,
  });
  await navigator.clipboard.writeText(command);
  showToast("Command copied", "success");
}

async function reconnectCurrent(): Promise<void> {
  const tab = currentTab();
  if (!tab) return;
  await invoke("reconnect_remote", { remote: tab.remote });
  showToast("Complete authentication in the Terminal and browser, then click Try again", "success");
}

async function checkRcloneUpdate(): Promise<void> {
  const info = await invoke<RcloneUpdateInfo>("check_rclone_update");
  state.dialog = { kind: "rclone-update", info };
  renderModal();
}

async function checkAppUpdate(silent = false): Promise<void> {
  try {
    const result = await invoke<UpdateStatus>("check_app_update");
    if (result.available) {
      state.dialog = {
        kind: "info",
        title: "Application update available",
        message: `${result.latestVersion} is available (installed: ${result.currentVersion}).`,
        details: result.releaseUrl,
      };
      renderModal();
    } else if (!silent) {
      showToast("Rclone Browser is up to date", "success");
    }
  } catch (error) {
    if (!silent) throw error;
  }
}

async function automaticUpdateChecks(): Promise<void> {
  if (state.settings?.checkAppUpdates) await checkAppUpdate(true);
  if (state.settings?.checkRcloneUpdates) {
    try {
      const info = await invoke<RcloneUpdateInfo>("check_rclone_update");
      if (info.stableUpdateAvailable && info.stable) {
        showToast(`rclone ${info.stable.version} is available.`, "success");
      }
    } catch {
      // Startup checks stay quiet; manual checks surface errors.
    }
  }
}

async function confirmQuit(): Promise<void> {
  const confirmed = await ask("Transfers, mounts, or streams are still running. Stop them and quit?", {
    title: "Rclone Browser", kind: "warning", okLabel: "Stop and quit", cancelLabel: "Keep running",
  });
  if (confirmed) await invoke("quit_app", { force: true });
}

async function saveSettings(showConfirmation = true): Promise<void> {
  if (!state.settings) return;
  const form = document.querySelector<HTMLFormElement>("#settings-form");
  if (!form) return;
  const data = new FormData(form);
  const settings: Settings = {
    rclonePath: String(data.get("rclonePath") ?? "").trim(),
    configPath: nullable(String(data.get("configPath") ?? "")),
    defaultDownloadDir: nullable(String(data.get("defaultDownloadDir") ?? "")),
    defaultUploadDir: nullable(String(data.get("defaultUploadDir") ?? "")),
    defaultDownloadArgs: lines(String(data.get("defaultDownloadArgs") ?? "")),
    defaultUploadArgs: lines(String(data.get("defaultUploadArgs") ?? "")),
    showHidden: data.get("showHidden") === "on",
    showFolderIcons: data.get("showFolderIcons") === "on",
    showFileIcons: data.get("showFileIcons") === "on",
    alternatingRows: data.get("alternatingRows") === "on",
    iconSize: String(data.get("iconSize") ?? "medium") as Settings["iconSize"],
    confirmDelete: data.get("confirmDelete") === "on",
    theme: String(data.get("theme") ?? "system") as Settings["theme"],
    advancedArgs: lines(String(data.get("advancedArgs") ?? "")),
    streamCommand: String(data.get("streamCommand") ?? "").trim(),
    mountArgs: lines(String(data.get("mountArgs") ?? "")),
    closeToTray: data.get("closeToTray") === "on",
    alwaysShowTray: data.get("alwaysShowTray") === "on",
    notifyFinishedTransfers: data.get("notifyFinishedTransfers") === "on",
    checkAppUpdates: data.get("checkAppUpdates") === "on",
    checkRcloneUpdates: data.get("checkRcloneUpdates") === "on",
    useProxy: data.get("useProxy") === "on",
    httpProxy: String(data.get("httpProxy") ?? "").trim(),
    httpsProxy: String(data.get("httpsProxy") ?? "").trim(),
    noProxy: String(data.get("noProxy") ?? "").trim(),
    exportOptions: state.settings.exportOptions,
    dualPane: data.get("dualPane") === "on",
    showTransferShelf: data.get("showTransferShelf") === "on",
    compactRows: data.get("compactRows") === "on",
  };
  await invoke("save_settings", { settings });
  await invoke("set_config_password", { password: String(data.get("configPassword") ?? "") });
  state.settings = settings;
  if (!settings.dualPane) state.activePane = "primary";
  applyTheme();
  applyAppearance();
  state.panes.primary.cache.clear();
  state.panes.secondary.cache.clear();
  if (settings.notifyFinishedTransfers) void ensureNotificationPermission();
  if (showConfirmation) showToast("Settings saved", "success");
}

async function choosePath(inputId: string, directory: boolean): Promise<void> {
  const result = await open({ directory, multiple: false });
  if (!result || Array.isArray(result)) return;
  const input = document.querySelector<HTMLInputElement>(`#${inputId}`);
  if (input) input.value = result;
}

function invalidateCurrent(): void {
  const tab = currentTab();
  if (tab) state.cache.delete(cacheKey(tab));
}

async function ensureNotificationPermission(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return await requestPermission() === "granted";
}

async function notifyTransferFinished(transfer: TransferSnapshot): Promise<void> {
  if (await ensureNotificationPermission()) {
    const title = transfer.status === "completed" ? "Transfer finished" : transfer.status === "failed" ? "Transfer failed" : "Transfer cancelled";
    sendNotification({ title, body: transfer.label ?? transfer.destination });
  }
}

function selectedEntry(): Entry | undefined {
  return state.entries.find((entry) => entry.path === state.selectedPath);
}

function setSort(key: SortKey): void {
  if (state.sort === key) state.sortAscending = !state.sortAscending;
  else { state.sort = key; state.sortAscending = true; }
}

function visibleEntries(): Entry[] {
  const search = state.search.trim().toLowerCase();
  const entries = state.entries.filter((entry) => entry.name.toLowerCase().includes(search));
  return entries.sort((left, right) => {
    if (left.isDir !== right.isDir) return left.isDir ? -1 : 1;
    let comparison = 0;
    if (state.sort === "name") comparison = left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
    if (state.sort === "size") comparison = (left.size ?? -1) - (right.size ?? -1);
    if (state.sort === "modified") comparison = (left.modTime ?? "").localeCompare(right.modTime ?? "");
    return state.sortAscending ? comparison : -comparison;
  });
}

function render(): void {
  root.innerHTML = `
    <div class="app-shell">${renderSidebar()}<main class="workspace" id="main-content">${renderMainContent()}</main></div>
    <div id="modal-root">${modalMarkup()}</div>
    <div class="toast-stack" id="toast-stack">${toastMarkup()}</div>
    <div id="drag-root">${dragOverlayMarkup()}</div>`;
}

function renderSidebar(): string {
  const running = state.transfers.filter(isActive).length + state.activities.filter(isActive).length;
  return `<aside class="sidebar">
    <div class="brand"><div class="brand-mark">${icon("cloud")}</div><div><strong>Rclone Browser</strong><span>Native workspace</span></div></div>
    <div class="sidebar-section"><div class="sidebar-heading"><span>Locations</span><div class="sidebar-heading-actions"><button class="icon-button small" data-action="manage-locations" title="Add or manage locations" aria-label="Add or manage locations">${icon("plus")}</button><button class="icon-button small" data-action="refresh-remotes" title="Reload remotes" aria-label="Reload remotes">${icon("refresh")}</button></div></div>
      <div class="remote-list">${state.remotes.map(remoteMarkup).join("") || `<p class="sidebar-empty">No remotes found</p>`}</div>
    </div>
    <div class="sidebar-section library-section"><div class="sidebar-heading"><span>Library</span></div><nav class="main-nav" aria-label="Library">
      ${navButton("transfers", "activity", "Activity", running)}${navButton("tasks", "tasks", "Saved Tasks", state.tasks.length)}${navButton("settings", "settings", "Settings")}
    </nav></div>
    <div class="connection ${state.rclone?.available ? "online" : "offline"}"><span class="status-dot"></span><div><strong>${state.rclone?.available ? "rclone connected" : "rclone needs attention"}</strong><span>${escapeHtml(state.rclone?.version ?? state.rclone?.error ?? "Check Settings")}</span></div></div>
  </aside>`;
}

function renderMainContent(): string {
  if (state.page === "settings") return settingsMarkup();
  if (state.page === "transfers") return transfersMarkup();
  if (state.page === "tasks") return tasksMarkup();
  if (!state.rclone?.available) return centeredState("alert", "rclone is unavailable", state.rclone?.error ?? "Choose the rclone executable in Settings.", "Open Settings", "nav-settings");
  if (!currentTab()) return centeredState("server", "Choose a location", "Open a remote or your local filesystem from the sidebar.", "Reload remotes", "refresh-remotes");
  return browserMarkup();
}

function renderMain(): void {
  const main = document.querySelector<HTMLElement>("#main-content");
  if (main) main.innerHTML = renderMainContent();
  renderTransferBadge();
}

function browserMarkup(): string {
  const selected = selectedEntry();
  const tab = currentTab()!;
  const dualPane = state.settings?.dualPane ?? true;
  return `<div class="browser-page"><div class="workspace-toolbar"><div class="navigation-buttons">
      <button class="icon-button small" data-action="nav-back" title="Back" aria-label="Back" ${tab.historyIndex <= 0 ? "disabled" : ""}>${icon("chevron-left")}</button>
      <button class="icon-button small" data-action="nav-forward" title="Forward" aria-label="Forward" ${tab.historyIndex >= tab.history.length - 1 ? "disabled" : ""}>${icon("chevron-right")}</button>
      <button class="icon-button small" data-action="nav-up" title="Parent folder" aria-label="Parent folder">${icon("arrow-up")}</button>
      <button class="icon-button small" data-action="refresh" title="Refresh" aria-label="Refresh">${icon("refresh")}</button>
    </div><div class="toolbar-breadcrumb">${breadcrumbMarkup()}</div><div class="workspace-transfer-actions">
      <button class="button secondary compact" data-action="copy-other-pane" ${!selected || !dualPane ? "disabled" : ""}>${icon("copy")} ${state.activePane === "primary" ? "Copy →" : "← Copy"}</button>
      <button class="button secondary compact" data-action="move-other-pane" ${!selected || !dualPane ? "disabled" : ""}>${icon("arrow-right")} ${state.activePane === "primary" ? "Move →" : "← Move"}</button>
      <button class="icon-button small" data-action="advanced-transfer" title="Transfer options" ${!selected || !dualPane ? "disabled" : ""}>${icon("sliders")}</button>
    </div><div class="workspace-file-actions">
      <button class="icon-button small" data-action="new-folder" title="New folder" ${tab.sharedWithMe ? "disabled" : ""}>${icon("folder-plus")}</button>
      <button class="icon-button small" data-action="upload" title="Upload" ${tab.sharedWithMe ? "disabled" : ""}>${icon("upload")}</button>
      <button class="icon-button small" data-action="download" title="Download" ${selected ? "" : "disabled"}>${icon("download")}</button>
      <button class="icon-button small" data-action="toggle-dual-pane" title="${dualPane ? "Use one pane" : "Use two panes"}">${icon(dualPane ? "panel-left" : "columns")}</button>
      <div class="overflow-wrap"><button class="icon-button small" data-action="toggle-menu" title="More actions">${icon("more")}</button>${state.menuOpen ? overflowMarkup(selected) : ""}</div>
    </div></div><div class="file-panes ${dualPane ? "dual" : "single"}">${browserPaneMarkup("primary")}${dualPane ? browserPaneMarkup("secondary") : ""}</div>${transferShelfMarkup()}</div>`;
}

function browserPaneMarkup(id: PaneId): string {
  return withPane(id, () => {
    const tab = currentTab();
    const remote = currentRemote();
    if (!tab) return `<section class="browser-pane ${state.activePane === id ? "active" : ""}" data-pane="${id}">${centeredState("server", "Choose a location", "Choose a location from the sidebar.", "", "")}</section>`;
    return `<section class="browser-pane ${state.activePane === id ? "active" : ""}" data-pane="${id}">${tabsMarkup()}<div class="pane-header"><div class="pane-location">${icon(remote?.isLocal ? "computer" : "server")}<strong>${escapeHtml(remote?.displayName || tab.remote)}</strong><span>${escapeHtml(tab.path || "/")}</span></div><div class="pane-actions">${remote?.type === "drive" ? `<label class="shared-toggle" title="Browse Google Drive Shared with me"><input type="checkbox" data-action="toggle-shared" ${tab.sharedWithMe ? "checked" : ""}><span>Shared with me</span></label>` : ""}<label class="pane-search">${icon("search")}<input data-file-search value="${escapeAttribute(state.search)}" placeholder="Filter" /></label></div></div><div data-table-region>${state.browserError ? remoteErrorMarkup() : tableMarkup()}</div></section>`;
  });
}

function tabsMarkup(): string {
  return `<div class="remote-tabs" role="tablist" aria-label="Open locations">${state.tabs.map((tab) => {
    const remote = state.remotes.find((item) => item.name === tab.remote);
    const active = tab.id === state.currentTabId;
    const location = lastPathPart(tab.path) || "Top level";
    return `<button class="remote-tab ${active ? "active" : ""}" data-action="select-tab" data-id="${tab.id}" role="tab" aria-selected="${active}" title="${escapeAttribute(`${remote?.displayName || tab.remote} · ${location}`)}">${icon("folder")}<span>${escapeHtml(location === "Top level" ? remote?.displayName || tab.remote : location)}</span>${state.tabs.length > 1 && active ? `<i data-action="close-tab" data-id="${tab.id}" title="Close ${escapeAttribute(remote?.displayName || tab.remote)}">${icon("x")}</i>` : ""}</button>`;
  }).join("")}<button class="icon-button small pane-new-tab" data-action="new-tab" title="New tab">${icon("plus")}</button></div>`;
}

function remoteErrorMarkup(): string {
  const summary = state.browserError?.split("\n").find((line) => line.trim()) ?? "The remote could not be opened.";
  const canReconnect = !currentRemote()?.isLocal;
  return `<section class="remote-error-state"><div class="remote-error-icon">${icon("alert")}</div><h1>Couldn’t open this remote</h1><p>${escapeHtml(summary)}</p>
    <div class="remote-error-actions">${canReconnect ? `<button class="button primary" data-action="reconnect">${icon("plug")} Reconnect</button>` : ""}<button class="button secondary" data-action="refresh">Try again</button><button class="button secondary" data-action="nav-settings">Open Settings</button></div>
    <details><summary>Technical details</summary><pre>${escapeHtml(state.browserError ?? "")}</pre></details></section>`;
}

function tableMarkup(): string {
  if (state.loading) return `<div class="file-panel"><div class="skeleton-head"></div>${Array.from({ length: 7 }, () => `<div class="skeleton-row"><i></i><span></span><em></em></div>`).join("")}</div>`;
  const entries = visibleEntries();
  if (!entries.length) {
    const searching = state.search.trim().length > 0;
    return `<div class="file-panel empty-table">${icon(searching ? "search" : "folder")}<h2>${searching ? "No matches" : "This folder is empty"}</h2><p>${searching ? "Try a different search." : "Drop files here, upload something, or create a folder."}</p></div>`;
  }
  return `<div class="file-panel"><table class="file-table"><thead><tr>${columnHeader("name", "Name")}${columnHeader("size", "Size")}${columnHeader("modified", "Modified")}<th aria-label="Type"></th></tr></thead><tbody>${entries.map(entryMarkup).join("")}</tbody></table><div class="table-footer"><span>${entries.length} item${entries.length === 1 ? "" : "s"}</span><span>${formatSize(entries.reduce((sum, entry) => sum + (entry.size ?? 0), 0))}</span></div></div>`;
}

function transferShelfMarkup(): string {
  if (!state.settings?.showTransferShelf || !state.transfers.length) return "";
  const visible = state.transfers.slice(0, 3);
  return `<section class="transfer-shelf"><div class="transfer-shelf-heading"><strong>Transfers</strong><button data-action="nav-transfers">View Activity</button></div><div class="transfer-shelf-items">${visible.map((transfer) => `<div class="transfer-shelf-item"><span class="status-dot ${isActive(transfer) ? "running" : ""}"></span><strong>${escapeHtml(transfer.label || `${capitalize(transfer.operation)} transfer`)}</strong><small>${statusLabel(transfer.status)}</small></div>`).join("")}</div></section>`;
}

function renderTable(): void {
  const region = document.querySelector<HTMLElement>(`[data-pane="${state.activePane}"] [data-table-region]`);
  if (region) region.innerHTML = tableMarkup();
}

function entryMarkup(entry: Entry): string {
  const selected = state.selectedPath === entry.path;
  const showIcon = entry.isDir ? state.settings?.showFolderIcons : state.settings?.showFileIcons;
  const iconMarkup = showIcon ? `<span class="file-icon ${entry.isDir ? "folder" : "file"}">${icon(entry.isDir ? "folder" : fileIcon(entry))}</span>` : "";
  return `<tr data-action="select-entry" data-entry-row data-path="${escapeAttribute(entry.path)}" class="${entry.isDir ? "directory " : ""}${selected ? "selected" : ""}" tabindex="0" aria-selected="${selected}"><td><div class="file-name">${iconMarkup}<span><strong>${escapeHtml(entry.name)}</strong><small>${entry.isDir ? "Folder · Double-click to open" : escapeHtml(fileKind(entry))}</small></span></div></td><td class="muted">${entry.isDir ? "—" : formatSize(entry.size ?? 0)}</td><td class="muted">${formatDate(entry.modTime)}</td><td class="row-chevron">${entry.isDir ? `<button data-action="open-entry" data-path="${escapeAttribute(entry.path)}" title="Open ${escapeAttribute(entry.name)}" aria-label="Open ${escapeAttribute(entry.name)}">${icon("chevron-right")}</button>` : ""}</td></tr>`;
}

function overflowMarkup(entry?: Entry): string {
  const folder = entry?.isDir || !entry;
  const writable = !currentTab()?.sharedWithMe;
  return `<div class="overflow-menu wide-menu">
    ${entry ? `${writable ? `<button data-action="rename">${icon("edit")} Rename</button><button data-action="move">${icon("move")} Move</button>` : ""}<button data-action="copy-path">${icon("copy")} Copy rclone path</button>${!currentRemote()?.isLocal ? `<button data-action="public-link">${icon("link")} Copy public link</button>` : ""}` : ""}
    <button data-action="advanced-transfer">${icon("sliders")} Advanced transfer</button><div></div>
    ${folder ? `<button data-action="get-size">${icon("database")} Calculate size</button><button data-action="get-tree">${icon("tree")} Directory tree</button><button data-action="export-txt">${icon("file")} Export filenames</button><button data-action="export-csv">${icon("table")} Export CSV</button><button data-action="mount">${icon("drive")} Mount folder</button>` : `<button data-action="stream">${icon("play")} Stream to player</button>`}
    ${entry && writable ? `<div></div><button class="destructive" data-action="delete">${icon("trash")} Delete ${entry.isDir ? "folder" : "file"}</button>` : ""}
  </div>`;
}

function transfersMarkup(): string {
  const activeTransfers = state.transfers.filter(isActive);
  const finishedTransfers = state.transfers.filter((item) => !isActive(item));
  const activeActivities = state.activities.filter(isActive);
  const finishedActivities = state.activities.filter((item) => !isActive(item));
  const anyFinished = finishedTransfers.length + finishedActivities.length > 0;
  return `${pageHeader("Activity", "Monitor transfers, mounts, and streams.", anyFinished ? `<button class="button secondary" data-action="clear-transfers">Clear finished</button>` : "")}
    <section class="content transfer-content">
      ${activeTransfers.length || activeActivities.length ? `<div class="section-label">Active <span>${activeTransfers.length + activeActivities.length}</span></div>${activeTransfers.map(transferMarkup).join("")}${activeActivities.map(activityMarkup).join("")}` : `<div class="quiet-card">${icon("check")}<div><strong>No active work</strong><span>Transfers, mounts, and streams will appear here.</span></div></div>`}
      ${anyFinished ? `<div class="section-label recent-label">Recent</div>${finishedTransfers.map(transferMarkup).join("")}${finishedActivities.map(activityMarkup).join("")}` : ""}
    </section>`;
}

function transferMarkup(transfer: TransferSnapshot): string {
  const active = isActive(transfer);
  const percent = transfer.totalBytes ? Math.min(100, (transfer.bytes / transfer.totalBytes) * 100) : 0;
  const name = transfer.label || lastPathPart(transfer.direction === "upload" ? transfer.source : transfer.destination) || "Transfer";
  const progress = active
    ? `<div class="progress-track"><i style="width:${percent}%"></i></div><div class="transfer-meta"><span>${formatSize(transfer.bytes)}${transfer.totalBytes ? ` of ${formatSize(transfer.totalBytes)}` : ""}</span><span>${transfer.speed ? `${formatSize(transfer.speed)}/s` : "Preparing…"}${transfer.etaSeconds ? ` · ${formatDuration(transfer.etaSeconds)} left` : ""}</span></div>`
    : `<div class="transfer-meta"><span>${formatSize(transfer.bytes)} transferred</span><span>${escapeHtml(transfer.error ?? formatTimestamp(transfer.finishedAt))}</span></div>`;
  return `<article class="transfer-card ${transfer.status}"><div class="transfer-icon">${icon(transfer.direction === "upload" ? "upload" : transfer.operation === "move" ? "move" : "download")}</div><div class="transfer-body"><div class="transfer-title"><div><strong>${escapeHtml(name)}</strong><span>${escapeHtml(transfer.source)} → ${escapeHtml(transfer.destination)}</span></div><span class="status-pill ${transfer.status}">${statusLabel(transfer.status)}</span></div>${progress}<details class="activity-details"><summary>Details, command, and output</summary><div class="activity-detail-actions"><button class="button secondary compact" data-action="copy-transfer-command" data-id="${transfer.id}">${icon("copy")} Copy command</button></div><div class="transfer-stats"><span><strong>Files</strong>${transfer.filesTransferred.toLocaleString()}${transfer.totalFiles !== null ? ` / ${transfer.totalFiles.toLocaleString()}` : ""}</span><span><strong>Checks</strong>${transfer.checks.toLocaleString()}${transfer.totalChecks !== null ? ` / ${transfer.totalChecks.toLocaleString()}` : ""}</span><span><strong>Errors</strong>${transfer.errors.toLocaleString()}</span><span><strong>Elapsed</strong>${transfer.elapsedSeconds !== null ? formatDuration(transfer.elapsedSeconds) : "—"}</span></div><pre>${escapeHtml(transfer.logTail.join("\n") || "No output yet.")}</pre></details></div>${active ? `<button class="icon-button" data-action="cancel-transfer" data-id="${transfer.id}" title="Cancel">${icon("x")}</button>` : ""}</article>`;
}

function activityMarkup(activity: ActivitySnapshot): string {
  const active = isActive(activity);
  return `<article class="transfer-card ${activity.status}"><div class="transfer-icon">${icon(activity.kind === "mount" ? "drive" : "play")}</div><div class="transfer-body"><div class="transfer-title"><div><strong>${activity.kind === "mount" ? "Mounted remote" : "Media stream"}</strong><span>${escapeHtml(activity.source)} → ${escapeHtml(activity.destination)}</span></div><span class="status-pill ${activity.status}">${statusLabel(activity.status)}</span></div><div class="transfer-meta"><span>${activity.kind === "mount" && active ? "Available until unmounted" : formatTimestamp(activity.finishedAt)}</span><span>${escapeHtml(activity.error ?? activity.logTail.at(-1) ?? "")}</span></div><details class="activity-details"><summary>Output</summary><pre>${escapeHtml(activity.logTail.join("\n") || activity.error || "No output yet.")}</pre></details></div>${active ? `<button class="button secondary compact" data-action="cancel-activity" data-id="${activity.id}">${activity.kind === "mount" ? "Unmount" : "Stop"}</button>` : ""}</article>`;
}

function tasksMarkup(): string {
  return `${pageHeader("Saved tasks", "Reusable copy, move, and sync jobs with full rclone options.", `<button class="button primary" data-action="new-task">${icon("plus")} New task</button>`)}<section class="content tasks-content">
    ${state.tasks.length ? `<div class="task-grid">${state.tasks.map(taskMarkup).join("")}</div>` : `<div class="empty-page-card">${icon("tasks")}<h2>No saved tasks</h2><p>Create a repeatable transfer or migrate existing Qt tasks automatically on launch.</p><button class="button primary" data-action="new-task">Create task</button></div>`}
  </section>`;
}

function taskMarkup(task: SavedTask): string {
  return `<article class="task-card"><div class="task-icon">${icon(task.direction === "upload" ? "upload" : task.direction === "download" ? "download" : "activity")}</div><div class="task-main"><div class="task-heading"><div><strong>${escapeHtml(task.description)}</strong><span>${capitalize(task.operation)} · ${task.isDirectory ? "Folder" : "File"}</span></div><button class="icon-button" data-action="edit-task" data-id="${task.id}" title="Edit">${icon("edit")}</button></div><div class="task-route"><code>${escapeHtml(task.source)}</code>${icon("arrow-right")}<code>${escapeHtml(task.destination)}</code></div><div class="task-actions"><button class="button primary" data-action="run-task" data-id="${task.id}">Run</button><button class="button secondary" data-action="dry-task" data-id="${task.id}">Dry run</button><button class="icon-button" data-action="copy-task-command" data-id="${task.id}" title="Copy command">${icon("copy")}</button><button class="icon-button danger-icon" data-action="delete-task" data-id="${task.id}" title="Delete">${icon("trash")}</button></div></div></article>`;
}

function settingsMarkup(): string {
  const settings = state.settings;
  if (!settings) return centeredState("settings", "Loading settings", "", "", "");
  return `${pageHeader("Settings", "The essentials first. Advanced controls stay out of the way.", `<button class="button primary" data-action="save-settings">${icon("check")} Save changes</button>`)}
    <section class="content settings-content"><form id="settings-form" class="settings-form">
      <nav class="settings-tabs" role="tablist" aria-label="Settings categories">
        ${settingsTabButton("general", "settings", "General")}
        ${settingsTabButton("connection", "plug", "Connection")}
        ${settingsTabButton("transfers", "activity", "Transfers")}
        ${settingsTabButton("advanced", "sliders", "Advanced")}
      </nav>
      <div class="settings-panes">
        ${settingsPane("general", `
          ${settingsStatusMarkup()}
          ${settingsSection("Appearance", "Match macOS or choose a fixed look.", `
            <div class="field-grid"><label class="field"><span>Theme</span><select name="theme"><option value="system" ${settings.theme === "system" ? "selected" : ""}>System</option><option value="light" ${settings.theme === "light" ? "selected" : ""}>Light</option><option value="dark" ${settings.theme === "dark" ? "selected" : ""}>Dark</option></select></label>
            <label class="field"><span>File icon size</span><select name="iconSize">${options(["small", "medium", "large"], settings.iconSize)}</select></label></div>`)}
          ${settingsSection("File browser", "Keep the file list useful without adding visual noise.", `
            ${toggleField("dualPane", "Use two file panes", "Browse a source and destination side by side.", settings.dualPane)}
            ${toggleField("showTransferShelf", "Show transfer shelf", "Keep recent transfers below the workspace.", settings.showTransferShelf)}
            ${toggleField("compactRows", "Compact file rows", "Fit more files in each pane.", settings.compactRows)}
            ${toggleField("showHidden", "Show hidden files", "Display dotfiles and hidden folders.", settings.showHidden)}
            ${toggleField("confirmDelete", "Confirm before deleting", "Ask before destructive operations.", settings.confirmDelete)}
            ${toggleField("showFolderIcons", "Show folder icons", "Display icons beside folders.", settings.showFolderIcons)}
            ${toggleField("showFileIcons", "Show file icons", "Display icons beside files.", settings.showFileIcons)}
            ${toggleField("alternatingRows", "Alternating row colours", "Use subtle striping in file lists.", settings.alternatingRows)}`)}
          ${settingsSection("Desktop", "Notifications, background behavior, and updates.", `
            ${toggleField("notifyFinishedTransfers", "Transfer notifications", "Notify when a running transfer finishes.", settings.notifyFinishedTransfers)}
            ${toggleField("closeToTray", "Close to tray", "Keep jobs running when the window closes.", settings.closeToTray)}
            ${toggleField("alwaysShowTray", "Always show tray icon", "Keep Rclone Browser available from the menu bar.", settings.alwaysShowTray)}
            ${toggleField("checkAppUpdates", "Check application updates", "Check for new Rclone Browser releases.", settings.checkAppUpdates)}
            ${toggleField("checkRcloneUpdates", "Check rclone updates", "Check the installed CLI version.", settings.checkRcloneUpdates)}
            <button type="button" class="button secondary fit" data-action="check-app-update">Check for updates</button>`)}
        `)}
        ${settingsPane("connection", `
          ${settingsSection("rclone", "The CLI remains the storage engine. Credentials stay in rclone’s configuration.", `
            ${pathField("rclone-path", "rclonePath", "Executable", settings.rclonePath, "choose-rclone", "Usually “rclone” when it is available on PATH.")}
            ${pathField("config-path", "configPath", "Configuration file", settings.configPath ?? "", "choose-config", "Leave blank to use rclone’s default configuration.")}
            <label class="field"><span>Configuration password</span><input name="configPassword" type="password" autocomplete="off" placeholder="Session only" /><small>Held in memory and never written to disk.</small></label>
            <div class="inline-actions connection-actions"><button type="button" class="button primary" data-action="test-rclone">${icon("plug")} Test connection</button><button type="button" class="button secondary" data-action="manage-locations">Manage locations</button><button type="button" class="button secondary" data-action="open-config">Terminal setup</button><button type="button" class="button tertiary" data-action="check-rclone-update">Check rclone update</button></div>`)}
        `)}
        ${settingsPane("transfers", `
          ${settingsSection("Default folders", "Used as the starting point for quick uploads and downloads.", `
            ${pathField("download-path", "defaultDownloadDir", "Downloads", settings.defaultDownloadDir ?? "", "choose-downloads", "Starting folder for downloaded files.")}
            ${pathField("upload-path", "defaultUploadDir", "Uploads", settings.defaultUploadDir ?? "", "choose-uploads", "Starting folder when choosing files to upload.")}`)}
          ${settingsSection("Transfer options", "Optional rclone arguments for quick actions.", `
            <details class="settings-disclosure"><summary><span><strong>Download arguments</strong><small>${settings.defaultDownloadArgs.length ? `${settings.defaultDownloadArgs.length} configured` : "Use rclone defaults"}</small></span>${icon("chevron-right")}</summary>${textareaField("defaultDownloadArgs", "Arguments", settings.defaultDownloadArgs)}</details>
            <details class="settings-disclosure"><summary><span><strong>Upload arguments</strong><small>${settings.defaultUploadArgs.length ? `${settings.defaultUploadArgs.length} configured` : "Use rclone defaults"}</small></span>${icon("chevron-right")}</summary>${textareaField("defaultUploadArgs", "Arguments", settings.defaultUploadArgs)}</details>`)}
          ${settingsSection("Mount and stream", "External integrations for mounted folders and media playback.", `
            <label class="field"><span>Player command</span><input name="streamCommand" value="${escapeAttribute(settings.streamCommand)}" placeholder="mpv -" /><small>The remote file is piped to standard input.</small></label>
            <details class="settings-disclosure"><summary><span><strong>Mount arguments</strong><small>${settings.mountArgs.length ? `${settings.mountArgs.length} configured` : "Use rclone defaults"}</small></span>${icon("chevron-right")}</summary>${textareaField("mountArgs", "Arguments", settings.mountArgs)}</details>`)}
        `)}
        ${settingsPane("advanced", `
          ${settingsSection("Proxy", "Only overrides system networking when enabled.", `
            ${toggleField("useProxy", "Use custom proxy", "Otherwise rclone inherits system environment settings.", settings.useProxy)}
            <div class="field-grid"><label class="field"><span>HTTP proxy</span><input name="httpProxy" value="${escapeAttribute(settings.httpProxy)}" /></label><label class="field"><span>HTTPS proxy</span><input name="httpsProxy" value="${escapeAttribute(settings.httpsProxy)}" /></label></div>
            <label class="field"><span>No proxy</span><input name="noProxy" value="${escapeAttribute(settings.noProxy)}" /></label>`)}
          ${settingsSection("Global arguments", "Applied to every rclone command. Change these only when needed.", textareaField("advancedArgs", "Arguments", settings.advancedArgs))}
          ${settingsSection("About", "Build and local data information.", `<div class="about-row"><span>Rclone Browser ${escapeHtml(state.appVersion)} · Rust + Tauri${state.portable ? " · Portable" : ""}</span><span title="${escapeAttribute(state.dataDirectory)}">${escapeHtml(shortenPath(state.dataDirectory))}</span></div>`)}
        `)}
      </div>
    </form></section>`;
}

function settingsSection(title: string, copy: string, fields: string): string {
  return `<section class="settings-section"><div class="settings-copy"><h2>${title}</h2><p>${copy}</p></div><div class="settings-fields">${fields}</div></section>`;
}

function settingsTabButton(tab: SettingsTab, iconName: string, label: string): string {
  const active = state.settingsTab === tab;
  return `<button type="button" class="settings-tab ${active ? "active" : ""}" data-action="select-settings-tab" data-settings-tab data-tab="${tab}" role="tab" aria-selected="${active}" aria-controls="settings-${tab}">${icon(iconName)}<span>${label}</span></button>`;
}

function settingsPane(tab: SettingsTab, content: string): string {
  return `<div class="settings-pane" id="settings-${tab}" role="tabpanel" ${state.settingsTab === tab ? "" : "hidden"}>${content}</div>`;
}

function selectSettingsTab(tab: SettingsTab): void {
  state.settingsTab = tab;
  document.querySelectorAll<HTMLElement>("[data-settings-tab]").forEach((button) => {
    const active = button.dataset.tab === tab;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", String(active));
  });
  document.querySelectorAll<HTMLElement>(".settings-pane").forEach((pane) => {
    pane.hidden = pane.id !== `settings-${tab}`;
  });
}

function settingsStatusMarkup(): string {
  const connected = state.rclone?.available === true;
  const remoteCount = state.remotes.filter((remote) => !remote.isLocal).length;
  const detail = connected
    ? `${state.rclone?.version ?? "rclone ready"} · ${remoteCount} location${remoteCount === 1 ? "" : "s"}`
    : state.rclone?.error || "Choose the rclone executable to get started.";
  return `<section class="settings-status ${connected ? "online" : "offline"}"><div class="settings-status-icon">${icon(connected ? "check" : "alert")}</div><div class="settings-status-copy"><span>Storage engine</span><strong>${connected ? "rclone connected" : "rclone needs attention"}</strong><small>${escapeHtml(detail)}</small></div><button type="button" class="button secondary" data-action="manage-locations">Manage locations</button></section>`;
}

function renderTransferBadge(): void {
  const running = state.transfers.filter(isActive).length + state.activities.filter(isActive).length;
  document.querySelectorAll<HTMLElement>("[data-transfer-badge]").forEach((badge) => {
    badge.textContent = running ? String(running) : "";
    badge.toggleAttribute("hidden", running === 0);
  });
}

function renderModal(): void {
  const container = document.querySelector<HTMLElement>("#modal-root");
  if (container) container.innerHTML = modalMarkup();
}

function modalMarkup(): string {
  const dialog = state.dialog;
  if (!dialog) return "";
  if (dialog.kind === "task") return taskDialogMarkup(dialog);
  if (dialog.kind === "export") return exportDialogMarkup(dialog);
  if (dialog.kind === "locations") return locationsDialogMarkup(dialog);
  if (dialog.kind === "rclone-update") return rcloneUpdateDialogMarkup(dialog);
  if (dialog.kind === "info") return `<div class="modal-backdrop"><section class="modal info-modal" role="dialog" aria-modal="true"><div class="modal-icon">${icon("info")}</div><h2>${escapeHtml(dialog.title)}</h2><p>${escapeHtml(dialog.message)}</p>${dialog.details ? `<pre class="dialog-output">${escapeHtml(dialog.details)}</pre>` : ""}<div class="modal-actions"><button class="button primary" data-action="close-modal">Done</button></div></section></div>`;
  return `<div class="modal-backdrop" data-action="close-modal"><section class="modal" role="dialog" aria-modal="true"><div class="modal-icon ${dialog.danger ? "danger" : ""}">${icon(dialog.danger ? "trash" : "folder-plus")}</div><h2>${escapeHtml(dialog.title)}</h2>${dialog.message ? `<p>${escapeHtml(dialog.message)}</p>` : ""}${dialog.label ? `<label class="field modal-field"><span>${escapeHtml(dialog.label)}</span><input id="modal-input" value="${escapeAttribute(dialog.value ?? "")}" autocomplete="off" /></label>` : ""}<div class="modal-actions"><button class="button secondary" data-action="close-modal">Cancel</button><button class="button ${dialog.danger ? "danger" : "primary"}" data-action="confirm-modal">${escapeHtml(dialog.confirmLabel)}</button></div></section></div>`;
}

function rcloneUpdateDialogMarkup(dialog: RcloneUpdateDialog): string {
  const { info } = dialog;
  const title = info.stableUpdateAvailable ? "rclone Update Available" : "rclone Is Up to Date";
  const releaseRow = (channel: "stable" | "beta", release: NonNullable<RcloneUpdateInfo["stable"]>) => {
    const recommended = channel === "stable" && info.stableUpdateAvailable;
    return `<div class="rclone-release-row"><div class="rclone-release-copy"><div><strong>${capitalize(channel)}</strong>${recommended ? `<span>RECOMMENDED</span>` : ""}</div><small>${release.released ? `Released ${escapeHtml(release.released)}` : "Release date unavailable"}</small></div><code title="${escapeAttribute(release.version)}">${escapeHtml(release.version)}</code><button class="button secondary" data-action="download-rclone" data-channel="${channel}" data-version="${escapeAttribute(release.version)}">Download</button></div>`;
  };
  return `<div class="modal-backdrop"><section class="modal rclone-update-modal" role="dialog" aria-modal="true" aria-labelledby="rclone-update-title"><div class="rclone-update-hero"><div class="rclone-update-icon ${info.stableUpdateAvailable ? "available" : "current"}">${icon(info.stableUpdateAvailable ? "arrow-down" : "check")}</div><h2 id="rclone-update-title">${title}</h2><p>Installed version ${escapeHtml(info.currentVersion)}</p></div><div class="rclone-release-list">${info.stable ? releaseRow("stable", info.stable) : ""}${info.beta ? releaseRow("beta", info.beta) : ""}</div><div class="rclone-update-footer"><small>Beta builds may be less stable.</small><button class="button primary" data-action="close-modal">Done</button></div></section></div>`;
}

function locationsDialogMarkup(dialog: LocationsDialog): string {
  const selected = dialog.providers.find((provider) => provider.name === dialog.provider);
  if (dialog.question || dialog.busy && dialog.provider) {
    return `<div class="modal-backdrop"><section class="modal locations-modal" role="dialog" aria-modal="true"><div class="modal-heading"><div><p class="eyebrow">RCLONE SETUP</p><h2>${escapeHtml(dialog.name || "New location")}</h2></div><button class="icon-button" data-action="cancel-location-config" aria-label="Close" ${dialog.busy ? "disabled" : ""}>${icon("x")}</button></div>
      <div class="config-provider-summary"><i class="remote-avatar">${icon("server")}</i><span><strong>${escapeHtml(selected?.description || dialog.provider)}</strong><small>${escapeHtml(dialog.provider)}</small></span></div>
      ${dialog.busy ? `<div class="config-busy"><span class="spinner"></span><strong>Waiting for rclone…</strong><p>An authorization page may open in your browser for OAuth providers.</p></div>` : configQuestionMarkup(dialog)}
    </section></div>`;
  }
  return `<div class="modal-backdrop"><section class="modal locations-modal" role="dialog" aria-modal="true"><div class="modal-heading"><div><p class="eyebrow">LOCATIONS</p><h2>Add a storage location</h2></div><button class="icon-button" data-action="cancel-location-config" aria-label="Close" ${dialog.busy ? "disabled" : ""}>${icon("x")}</button></div>
    <p class="location-intro">Choose from every backend reported by your installed rclone. The next steps are generated by rclone itself, including provider-specific credentials and OAuth.</p>
    ${dialog.busy ? `<div class="config-busy"><span class="spinner"></span><strong>Loading rclone protocols…</strong></div>` : `<form id="location-form" class="location-form"><label class="field location-name-field"><span>Location name</span><input id="location-name" name="name" value="${escapeAttribute(dialog.name)}" placeholder="For example: Work Drive" autocomplete="off" /><small>A short label used in paths such as Work Drive:Documents.</small></label><div class="provider-search-block"><div class="provider-search-heading"><span>Storage protocol</span><small id="provider-count">${configProviderCount(dialog)}</small></div><div class="search-input">${icon("search")}<input id="location-search" name="search" value="${escapeAttribute(dialog.search)}" placeholder="Search Google Drive, S3, SFTP, WebDAV…" autocomplete="off" /></div></div></form>
      <div class="provider-list" id="provider-list">${configProviderListMarkup(dialog)}</div>`}
    <div class="modal-actions locations-actions"><button class="button secondary terminal-action" data-action="location-config-terminal">${icon("sliders")} Full setup in Terminal</button><button class="button secondary" data-action="cancel-location-config" ${dialog.busy ? "disabled" : ""}>Cancel</button><button class="button primary" data-action="start-location-config" ${dialog.busy || !dialog.provider ? "disabled" : ""}>Continue</button></div>
  </section></div>`;
}

function configProviderCount(dialog: LocationsDialog): string {
  const visible = filteredConfigProviders(dialog);
  return `${visible.length} of ${dialog.providers.length} protocols`;
}

function filteredConfigProviders(dialog: LocationsDialog): ConfigProvider[] {
  const needle = dialog.search.trim().toLowerCase();
  if (!needle) return dialog.providers;
  return dialog.providers.filter((provider) => `${provider.description} ${provider.name} ${provider.prefix}`.toLowerCase().includes(needle));
}

function configProviderListMarkup(dialog: LocationsDialog): string {
  const providers = filteredConfigProviders(dialog);
  if (!providers.length) return `<div class="provider-empty">No matching rclone protocol.</div>`;
  return providers.map((provider) => `<button type="button" class="provider-option ${dialog.provider === provider.name ? "selected" : ""}" data-action="select-provider" data-provider="${escapeAttribute(provider.name)}"><i>${icon("server")}</i><span><strong>${escapeHtml(provider.description)}</strong><small>${escapeHtml(provider.name)}</small></span>${dialog.provider === provider.name ? icon("check") : ""}</button>`).join("");
}

function configQuestionMarkup(dialog: LocationsDialog): string {
  const option = dialog.question?.option;
  if (!option) return `<div class="config-busy"><strong>Finishing configuration…</strong></div>`;
  const value = option.valueStr || option.defaultStr;
  let control: string;
  if (option.exclusive && option.examples.length) {
    control = `<select id="location-answer">${option.examples.map((example) => `<option value="${escapeAttribute(example.value)}" ${example.value === value ? "selected" : ""}>${escapeHtml(example.value)}${example.help ? ` — ${escapeHtml(example.help.split("\n")[0])}` : ""}</option>`).join("")}</select>`;
  } else if (option.optionType.toLowerCase() === "bool") {
    control = `<select id="location-answer"><option value="true" ${value === "true" ? "selected" : ""}>Yes</option><option value="false" ${value !== "true" ? "selected" : ""}>No</option></select>`;
  } else {
    const list = option.examples.length ? ` list="config-answer-options"` : "";
    control = `<input id="location-answer" type="${option.isPassword ? "password" : "text"}" value="${escapeAttribute(value)}"${list} ${option.required ? "required" : ""} autocomplete="${option.isPassword ? "new-password" : "off"}" />${option.examples.length ? `<datalist id="config-answer-options">${option.examples.map((example) => `<option value="${escapeAttribute(example.value)}">${escapeHtml(example.help.split("\n")[0])}</option>`).join("")}</datalist>` : ""}`;
  }
  return `<div class="config-question"><div class="config-step"><span>Provider question</span><strong>${escapeHtml(friendlyConfigName(option.name))}${option.required ? " *" : ""}</strong></div><p class="config-help">${escapeHtml(option.help || "Enter the value requested by rclone.")}</p><label class="field"><span>${escapeHtml(friendlyConfigName(option.name))}</span>${control}${option.sensitive && !option.isPassword ? `<small>This value may contain sensitive account information.</small>` : ""}</label><div class="modal-actions"><button class="button secondary" data-action="cancel-location-config">Cancel setup</button><button class="button primary" data-action="continue-location-config">Continue</button></div></div>`;
}

function friendlyConfigName(name: string): string {
  return name.replace(/^config_/, "").replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function exportDialogMarkup(dialog: ExportDialog): string {
  const o = dialog.options;
  return `<div class="modal-backdrop"><section class="modal export-modal" role="dialog" aria-modal="true"><div class="modal-heading"><div><p class="eyebrow">EXPORT</p><h2>${dialog.format === "csv" ? "CSV file list" : "Filename list"}</h2></div><button class="icon-button" data-action="close-modal">${icon("x")}</button></div>
    <form id="export-form" class="task-form"><label class="field span-2"><span>Destination file</span><div class="path-input"><input id="export-destination" name="destination" value="${escapeAttribute(dialog.destination)}" /><button type="button" class="icon-button" data-action="choose-export-destination" title="Choose destination">${icon("folder")}</button></div></label>
      <div class="span-2 task-switches">${checkbox("oneFileSystem", "Do not cross filesystem boundaries", o.oneFileSystem)}</div>
      <label class="field"><span>Minimum size</span><input name="minSize" value="${escapeAttribute(o.minSize)}" placeholder="10M" /></label><label class="field"><span>Maximum depth (0 = all)</span><input type="number" min="0" name="maxDepth" value="${o.maxDepth}" /></label>
      <label class="field"><span>Minimum age</span><input name="minAge" value="${escapeAttribute(o.minAge)}" placeholder="1h" /></label><label class="field"><span>Maximum age</span><input name="maxAge" value="${escapeAttribute(o.maxAge)}" placeholder="30d" /></label>
      <label class="field"><span>Excludes (one per line)</span><textarea name="excludes" rows="4">${escapeHtml(o.excludes.join("\n"))}</textarea></label><label class="field"><span>Extra arguments (one per line)</span><textarea name="extraArgs" rows="4">${escapeHtml(o.extraArgs.join("\n"))}</textarea></label>
    </form><div class="modal-actions"><button class="button secondary reset-action" data-action="export-reset">Reset options</button><button class="button secondary" data-action="close-modal">Cancel</button><button class="button primary" data-action="export-confirm">Export</button></div></section></div>`;
}

function taskDialogMarkup(dialog: TaskDialog): string {
  const t = dialog.task;
  return `<div class="modal-backdrop"><section class="modal task-modal" role="dialog" aria-modal="true"><div class="modal-heading"><div><p class="eyebrow">TRANSFER TASK</p><h2>${escapeHtml(dialog.title)}</h2></div><button class="icon-button" data-action="close-modal">${icon("x")}</button></div><form id="task-form" class="task-form">
    <label class="field span-2"><span>Description</span><input name="description" value="${escapeAttribute(t.description)}" placeholder="Nightly archive backup" /></label>
    <label class="field"><span>Direction</span><select name="direction">${options(["copy", "upload", "download"], t.direction)}</select></label><label class="field"><span>Operation</span><select name="operation">${options(["copy", "move", "sync"], t.operation)}</select></label>
    <label class="field span-2"><span>Source</span><div class="path-input"><input name="source" value="${escapeAttribute(t.source)}" placeholder="remote:path or local path" /><button type="button" class="icon-button" data-action="choose-task-source" title="Choose local source">${icon("folder")}</button></div></label><label class="field span-2"><span>Destination</span><div class="path-input"><input name="destination" value="${escapeAttribute(t.destination)}" placeholder="remote:path or local path" /><button type="button" class="icon-button" data-action="choose-task-destination" title="Choose local destination">${icon("folder")}</button></div></label>
    <div class="span-2 task-switches">${checkbox("isDirectory", "Source is a folder", t.isDirectory)}${checkbox("sharedWithMe", "Google Shared with me", t.sharedWithMe)}${checkbox("update", "Skip newer destination files", t.update)}${checkbox("ignoreExisting", "Skip existing files", t.ignoreExisting)}</div>
    <details class="advanced-task span-2"><summary>Advanced options</summary><div class="advanced-task-grid">
      <label class="field"><span>Sync deletion</span><select name="syncDeleteMode"><option value="">Default</option>${options(["during", "after", "before"], t.syncDeleteMode ?? "")}</select></label><label class="field"><span>Comparison</span><select name="compareMode">${options(["sizeAndModTime", "checksum", "ignoreSize", "sizeOnly", "checksumIgnoreSize"], t.compareMode)}</select></label>
      ${numberField("transfers", "Transfers", t.transfers)}${numberField("checkers", "Checkers", t.checkers)}${numberField("maxDepth", "Max depth (0 = all)", t.maxDepth)}${numberField("retries", "Retries", t.retries)}${numberField("lowLevelRetries", "Low-level retries", t.lowLevelRetries)}${numberField("connectTimeoutSeconds", "Connect timeout (s)", t.connectTimeoutSeconds)}${numberField("idleTimeoutSeconds", "Idle timeout (s)", t.idleTimeoutSeconds)}
      <label class="field"><span>Bandwidth limit</span><input name="bandwidth" value="${escapeAttribute(t.bandwidth)}" placeholder="10M" /></label><label class="field"><span>Minimum size</span><input name="minSize" value="${escapeAttribute(t.minSize)}" /></label><label class="field"><span>Minimum age</span><input name="minAge" value="${escapeAttribute(t.minAge)}" /></label><label class="field"><span>Maximum age</span><input name="maxAge" value="${escapeAttribute(t.maxAge)}" /></label>
      <div class="span-2 task-switches">${checkbox("oneFileSystem", "One filesystem", t.oneFileSystem)}${checkbox("noUpdateModtime", "Do not update modified time", t.noUpdateModtime)}${checkbox("deleteExcluded", "Delete excluded files", t.deleteExcluded)}</div>
      <label class="field"><span>Excludes (one per line)</span><textarea name="excludes" rows="4">${escapeHtml(t.excludes.join("\n"))}</textarea></label><label class="field"><span>Extra arguments (one per line)</span><textarea name="extraArgs" rows="4">${escapeHtml(t.extraArgs.join("\n"))}</textarea></label>
    </div></details>
  </form><div class="modal-actions task-modal-actions"><button class="button secondary reset-action" data-action="task-reset">Reset options</button><button class="button secondary" data-action="close-modal">Cancel</button><button class="button secondary" data-action="task-save">Save task</button><button class="button secondary" data-action="task-dry">Dry run</button><button class="button primary" data-action="task-run">Run</button></div></section></div>`;
}

function showToast(message: string, kind: Toast["kind"]): void {
  const id = ++toastSequence;
  state.toasts.push({ id, message, kind });
  renderToasts();
  window.setTimeout(() => { state.toasts = state.toasts.filter((toast) => toast.id !== id); renderToasts(); }, 4600);
}

function renderToasts(): void {
  const container = document.querySelector<HTMLElement>("#toast-stack");
  if (container) container.innerHTML = toastMarkup();
}

function toastMarkup(): string {
  return state.toasts.map((toast) => `<div class="toast ${toast.kind}">${icon(toast.kind === "success" ? "check" : "alert")}<span>${escapeHtml(toast.message)}</span></div>`).join("");
}

function renderDragOverlay(): void {
  const container = document.querySelector<HTMLElement>("#drag-root");
  if (container) container.innerHTML = dragOverlayMarkup();
}

function dragOverlayMarkup(): string {
  return state.dragActive && state.page === "browser" && currentTab() && !currentTab()!.sharedWithMe ? `<div class="drop-overlay"><div>${icon("upload")}<strong>Drop to upload</strong><span>${escapeHtml(currentTab()!.path || currentRemote()?.displayName || "this location")}</span></div></div>` : "";
}

function applyTheme(): void { document.documentElement.dataset.theme = state.settings?.theme ?? "system"; }
function applyAppearance(): void {
  document.documentElement.dataset.iconSize = state.settings?.iconSize ?? "medium";
  document.documentElement.dataset.alternatingRows = String(state.settings?.alternatingRows ?? true);
  document.documentElement.dataset.compactRows = String(state.settings?.compactRows ?? true);
}

function navButton(page: Page, iconName: string, label: string, badge = 0): string {
  return `<button class="nav-item ${state.page === page ? "active" : ""}" data-action="nav-${page}">${icon(iconName)}<span>${label}</span>${badge ? `<em ${page === "transfers" ? "data-transfer-badge" : ""}>${badge}</em>` : page === "transfers" ? `<em data-transfer-badge hidden></em>` : ""}</button>`;
}

function remoteMarkup(remote: Remote): string {
  const active = currentTab()?.remote === remote.name && state.page === "browser";
  return `<button class="remote-item ${active ? "active" : ""}" data-action="select-remote" data-remote="${escapeAttribute(remote.name)}"><i class="remote-avatar type-${escapeAttribute(remote.type)}">${icon(remote.isLocal ? "computer" : "server")}</i><span><strong>${escapeHtml(remote.displayName || remote.name)}</strong>${remote.isLocal ? "" : `<small>${escapeHtml(remote.type)}</small>`}</span>${active ? icon("chevron-right") : ""}</button>`;
}

function breadcrumbMarkup(): string {
  const tab = currentTab();
  if (!tab) return "";
  const remote = currentRemote();
  return `<button data-action="open-path" data-path="">${icon(remote?.isLocal ? "computer" : "server")} ${escapeHtml(remote?.displayName || tab.remote)}</button>${tab.path ? `<span>${icon("chevron-right")}</span><button data-action="open-path" data-path="${escapeAttribute(tab.path)}" title="${escapeAttribute(tab.path)}">${escapeHtml(tab.path)}</button>` : ""}`;
}

function columnHeader(key: SortKey, label: string): string {
  return `<th><button data-action="sort" data-sort="${key}">${label}${state.sort === key ? icon(state.sortAscending ? "arrow-up" : "arrow-down") : ""}</button></th>`;
}

function pageHeader(title: string, subtitle: string, actions: string): string {
  return `<header class="page-header"><div><h1>${title}</h1><p>${subtitle}</p></div><div>${actions}</div></header>`;
}

function centeredState(iconName: string, title: string, message: string, actionLabel: string, action: string): string {
  return `<section class="centered-state"><div>${icon(iconName)}</div><h1>${escapeHtml(title)}</h1><p>${escapeHtml(message)}</p>${actionLabel ? `<button class="button primary" data-action="${action}">${escapeHtml(actionLabel)}</button>` : ""}</section>`;
}

function pathField(id: string, name: string, label: string, value: string, action: string, hint: string): string {
  return `<label class="field"><span>${label}</span><div class="path-input"><input id="${id}" name="${name}" value="${escapeAttribute(value)}" /><button type="button" class="icon-button" data-action="${action}" title="Browse">${icon("folder")}</button></div><small>${hint}</small></label>`;
}

function textareaField(name: string, label: string, value: string[]): string {
  return `<label class="field"><span>${label}</span><textarea name="${name}" rows="3">${escapeHtml(value.join("\n"))}</textarea><small>One complete argument per line.</small></label>`;
}

function toggleField(name: string, label: string, hint: string, checked: boolean): string {
  return `<label class="toggle-field"><span><strong>${label}</strong><small>${hint}</small></span><input name="${name}" type="checkbox" ${checked ? "checked" : ""} /><i></i></label>`;
}

function checkbox(name: string, label: string, checked: boolean): string {
  return `<label class="mini-check"><input type="checkbox" name="${name}" ${checked ? "checked" : ""}><span>${label}</span></label>`;
}

function numberField(name: string, label: string, value: number): string {
  return `<label class="field"><span>${label}</span><input type="number" min="0" name="${name}" value="${value}"></label>`;
}

function options(values: string[], selected: string): string {
  return values.map((value) => `<option value="${value}" ${value === selected ? "selected" : ""}>${friendlyOption(value)}</option>`).join("");
}

function friendlyOption(value: string): string {
  return ({ sizeAndModTime: "Size and modified time", checksum: "Checksum", ignoreSize: "Ignore size", sizeOnly: "Size only", checksumIgnoreSize: "Checksum + ignore size" } as Record<string, string>)[value] ?? capitalize(value);
}

function focusModalInput(select = false): void {
  window.requestAnimationFrame(() => { const input = document.querySelector<HTMLInputElement>("#modal-input"); input?.focus(); if (select) input?.select(); });
}

function fileKind(entry: Entry): string {
  if (entry.mimeType) return entry.mimeType.split("/").pop()?.replaceAll("-", " ") ?? "File";
  const extension = entry.name.split(".").pop();
  return extension && extension !== entry.name ? `${extension.toUpperCase()} file` : "File";
}

function fileIcon(entry: Entry): string {
  const mime = entry.mimeType ?? "";
  if (mime.startsWith("image/")) return "image";
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("audio/")) return "music";
  if (mime.includes("zip") || mime.includes("archive")) return "archive";
  return "file";
}

function lastPathPart(path: string): string { return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path; }
function shortenPath(path: string): string { return path.length > 42 ? `…${path.slice(-41)}` : path; }
function capitalize(value: string): string { return value ? value[0].toUpperCase() + value.slice(1) : value; }
function lines(value: string): string[] { return value.split("\n").map((line) => line.trim()).filter(Boolean); }
function isActive(item: { status: string }): boolean { return item.status === "queued" || item.status === "running"; }

function upsertById<T extends { id: string }>(items: T[], payload: T): void {
  const index = items.findIndex((item) => item.id === payload.id);
  if (index === -1) items.unshift(payload); else items[index] = payload;
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toLocaleString(undefined, { maximumFractionDigits: index ? 1 : 0 })} ${units[index]}`;
}

function formatDate(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function formatTimestamp(value: number | null): string { return value ? formatDate(new Date(value * 1000).toISOString()) : ""; }
function formatDuration(seconds: number): string { return seconds < 60 ? `${Math.ceil(seconds)} sec` : seconds < 3600 ? `${Math.ceil(seconds / 60)} min` : `${(seconds / 3600).toFixed(1)} hr`; }
function statusLabel(status: TransferSnapshot["status"]): string { return ({ queued: "Queued", running: "In progress", completed: "Completed", failed: "Failed", cancelled: "Cancelled" })[status]; }
function nullable(value: string): string | null { const trimmed = value.trim(); return trimmed || null; }
function errorMessage(error: unknown): string { return error instanceof Error ? error.message : String(error); }
function escapeHtml(value: string): string { return value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character] ?? character); }
function escapeAttribute(value: string): string { return escapeHtml(value); }

function icon(name: string): string {
  const paths: Record<string, string> = {
    cloud: '<path d="M17.5 19H9a7 7 0 1 1 6.7-9h1.8a4.5 4.5 0 1 1 0 9Z"/>', folder: '<path d="M3 6.5A2.5 2.5 0 0 1 5.5 4H9l2 2h7.5A2.5 2.5 0 0 1 21 8.5v8A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"/>',
    activity: '<path d="M3 12h4l2.5-7 5 14 2.5-7h4"/>', tasks: '<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 8h8M8 12h8M8 16h5"/>', settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6 1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/>',
    refresh: '<path d="M20 6v5h-5"/><path d="M19 11a7.5 7.5 0 1 0 .2 3"/>', search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>', upload: '<path d="m12 16 0-11m-4 4 4-4 4 4"/><path d="M5 18v2h14v-2"/>', download: '<path d="m12 5 0 11m-4-4 4 4 4-4"/><path d="M5 20h14"/>', "folder-plus": '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/><path d="M12 10v6m-3-3h6"/>', "folder-up": '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/><path d="m12 16 0-6m-3 3 3-3 3 3"/>',
    more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>', server: '<rect x="3" y="4" width="18" height="6" rx="2"/><rect x="3" y="14" width="18" height="6" rx="2"/><path d="M7 7h.01M7 17h.01"/>', computer: '<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>', "chevron-left": '<path d="m15 18-6-6 6-6"/>', "chevron-right": '<path d="m9 18 6-6-6-6"/>', edit: '<path d="M12 20h9"/><path d="m16.5 3.5 4 4L8 20H4v-4Z"/>', move: '<path d="M5 9V5h4M19 15v4h-4M9 5l10 10M5 19l5-5"/>', copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>', link: '<path d="M10 13a5 5 0 0 0 7.5.5l2-2a5 5 0 0 0-7-7l-1.2 1.2"/><path d="M14 11a5 5 0 0 0-7.5-.5l-2 2a5 5 0 0 0 7 7l1.2-1.2"/>', trash: '<path d="M4 7h16M9 7V4h6v3m3 0-1 14H7L6 7m4 4v6m4-6v6"/>',
    "arrow-up": '<path d="m8 11 4-4 4 4m-4-4v10"/>', "arrow-down": '<path d="m8 13 4 4 4-4m-4 4V7"/>', "arrow-right": '<path d="M5 12h14m-5-5 5 5-5 5"/>', file: '<path d="M6 2h8l4 4v16H6Z"/><path d="M14 2v5h5"/>', image: '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/>', video: '<rect x="3" y="5" width="15" height="14" rx="2"/><path d="m18 10 4-2v8l-4-2Z"/>', music: '<path d="M9 18V5l10-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>', archive: '<path d="M6 3h12v18H6Zm4 0v3h4V3m-4 6h4m-4 3h4"/>',
    check: '<path d="m5 12 4 4L19 6"/>', x: '<path d="m6 6 12 12M18 6 6 18"/>', alert: '<path d="M12 3 2.5 20h19Z"/><path d="M12 9v4m0 3h.01"/>', plug: '<path d="m8 12 8-8m-5 11 8-8M6 9l9 9m-6 3 3-3M3 12l3-3"/>', sliders: '<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="9" cy="6" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="7" cy="18" r="2"/>', database: '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v6c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 11v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6"/>', tree: '<path d="M12 3v6m0 0H6v5m6-5h6v5M6 14v4m12-4v4"/><rect x="3" y="18" width="6" height="3" rx="1"/><rect x="15" y="18" width="6" height="3" rx="1"/>', table: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18M9 4v16m6-11v11"/>', drive: '<path d="M4 14 7 4h10l3 10v5H4Z"/><path d="M4 14h16M8 17h.01"/>', play: '<path d="m8 5 11 7-11 7Z"/>', plus: '<path d="M12 5v14M5 12h14"/>', info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-9h.01"/>',
    "panel-left": '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>', columns: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>',
  };
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[name] ?? paths.file}</svg>`;
}
