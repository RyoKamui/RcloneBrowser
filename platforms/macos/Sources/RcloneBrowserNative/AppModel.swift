import AppKit
import Combine
import Foundation
import UserNotifications

@MainActor
final class PaneState: ObservableObject, Identifiable {
    let id: PaneID
    @Published var tabs: [BrowserTab]
    @Published var activeTabID: UUID
    @Published var entries: [BrowserEntry] = [] {
        didSet { rebuildVisibleEntries() }
    }
    @Published private(set) var visibleEntries: [BrowserEntry] = []
    @Published var selectedIDs: Set<String> = []
    @Published var isLoading = false
    @Published var search = "" {
        didSet { rebuildVisibleEntries() }
    }
    @Published var error: String?
    @Published var sort: FileSort = .name {
        didSet { rebuildVisibleEntries() }
    }
    @Published var sortAscending = true {
        didSet { rebuildVisibleEntries() }
    }

    private var histories: [UUID: [BrowserLocation]]
    private var historyIndices: [UUID: Int]
    private var entryCache: [BrowserLocation: [BrowserEntry]] = [:]
    var requestToken = UUID()

    init(id: PaneID, remote: String, path: String) {
        self.id = id
        let tab = BrowserTab(remote: remote, path: path)
        tabs = [tab]
        activeTabID = tab.id
        histories = [tab.id: [BrowserLocation(remote: remote, path: path)]]
        historyIndices = [tab.id: 0]
    }

    var activeTab: BrowserTab {
        tabs.first(where: { $0.id == activeTabID }) ?? tabs[0]
    }

    var remote: String { activeTab.remote }
    var path: String { activeTab.path }
    var sharedWithMe: Bool { activeTab.sharedWithMe }
    var canGoBack: Bool { activeHistoryIndex > 0 }
    var canGoForward: Bool { activeHistoryIndex + 1 < activeHistory.count }
    var selectedEntries: [BrowserEntry] { entries.filter { selectedIDs.contains($0.id) } }

    func cachedEntries() -> [BrowserEntry]? {
        entryCache[BrowserLocation(remote: remote, path: path, sharedWithMe: sharedWithMe)]
    }

    func cache(_ entries: [BrowserEntry]) {
        entryCache[BrowserLocation(remote: remote, path: path, sharedWithMe: sharedWithMe)] = entries
    }

    func clearCache() {
        entryCache.removeAll()
    }

    private func rebuildVisibleEntries() {
        let filtered = search.isEmpty ? entries : entries.filter { $0.name.localizedStandardContains(search) }
        let sorted = filtered.sorted { left, right in
            if left.isDir != right.isDir { return left.isDir }
            let comparison: ComparisonResult
            switch sort {
            case .name:
                comparison = left.name.localizedStandardCompare(right.name)
            case .size:
                comparison = (left.size ?? 0) == (right.size ?? 0)
                    ? left.name.localizedStandardCompare(right.name)
                    : ((left.size ?? 0) < (right.size ?? 0) ? .orderedAscending : .orderedDescending)
            case .modified:
                comparison = (left.modTime ?? "") == (right.modTime ?? "")
                    ? left.name.localizedStandardCompare(right.name)
                    : ((left.modTime ?? "") < (right.modTime ?? "") ? .orderedAscending : .orderedDescending)
            }
            return sortAscending ? comparison == .orderedAscending : comparison == .orderedDescending
        }
        if visibleEntries != sorted { visibleEntries = sorted }
    }

    func navigate(remote: String, path: String, recordHistory: Bool = true) {
        let keepSharedWithMe = remote == self.remote ? sharedWithMe : false
        updateActiveTab(remote: remote, path: path, sharedWithMe: keepSharedWithMe)
        selectedIDs.removeAll()
        if recordHistory {
            var history = activeHistory
            var index = activeHistoryIndex
            if index + 1 < history.count { history.removeSubrange((index + 1)..<history.count) }
            let location = BrowserLocation(remote: remote, path: path, sharedWithMe: sharedWithMe)
            if history.last != location { history.append(location) }
            index = history.count - 1
            histories[activeTabID] = history
            historyIndices[activeTabID] = index
        }
    }

    func goBack() -> BrowserLocation? {
        guard canGoBack else { return nil }
        let index = activeHistoryIndex - 1
        historyIndices[activeTabID] = index
        let location = activeHistory[index]
        updateActiveTab(remote: location.remote, path: location.path, sharedWithMe: location.sharedWithMe)
        selectedIDs.removeAll()
        return location
    }

    func goForward() -> BrowserLocation? {
        guard canGoForward else { return nil }
        let index = activeHistoryIndex + 1
        historyIndices[activeTabID] = index
        let location = activeHistory[index]
        updateActiveTab(remote: location.remote, path: location.path, sharedWithMe: location.sharedWithMe)
        selectedIDs.removeAll()
        return location
    }

    func newTab(remote: String? = nil, path: String? = nil) {
        let nextRemote = remote ?? self.remote
        let tab = BrowserTab(remote: nextRemote, path: path ?? self.path, sharedWithMe: nextRemote == self.remote ? sharedWithMe : false)
        tabs.append(tab)
        activeTabID = tab.id
        histories[tab.id] = [BrowserLocation(remote: tab.remote, path: tab.path, sharedWithMe: tab.sharedWithMe)]
        historyIndices[tab.id] = 0
        selectedIDs.removeAll()
    }

    func selectTab(_ id: UUID) {
        guard tabs.contains(where: { $0.id == id }) else { return }
        activeTabID = id
        selectedIDs.removeAll()
    }

    func closeTab(_ id: UUID) {
        guard tabs.count > 1, let index = tabs.firstIndex(where: { $0.id == id }) else { return }
        tabs.remove(at: index)
        histories[id] = nil
        historyIndices[id] = nil
        if activeTabID == id { activeTabID = tabs[min(index, tabs.count - 1)].id }
        selectedIDs.removeAll()
    }

    func toggleSharedWithMe() {
        guard let index = tabs.firstIndex(where: { $0.id == activeTabID }) else { return }
        tabs[index].sharedWithMe.toggle()
        var history = activeHistory
        if history.indices.contains(activeHistoryIndex) {
            history[activeHistoryIndex].sharedWithMe = tabs[index].sharedWithMe
            histories[activeTabID] = history
        }
        selectedIDs.removeAll()
    }

    private var activeHistory: [BrowserLocation] {
        histories[activeTabID] ?? [BrowserLocation(remote: remote, path: path, sharedWithMe: sharedWithMe)]
    }

    private var activeHistoryIndex: Int {
        min(historyIndices[activeTabID] ?? 0, max(activeHistory.count - 1, 0))
    }

    private func updateActiveTab(remote: String, path: String, sharedWithMe: Bool? = nil) {
        guard let index = tabs.firstIndex(where: { $0.id == activeTabID }) else { return }
        tabs[index].remote = remote
        tabs[index].path = path
        if let sharedWithMe { tabs[index].sharedWithMe = sharedWithMe }
    }
}

@MainActor
final class AppModel: ObservableObject {
    @Published var settings = AppSettings.fallback {
        didSet { StatusBarController.shared.apply(settings) }
    }
    @Published var appVersion = "3.0.0"
    @Published var rclone = RcloneStatus(available: false, version: nil, error: nil)
    @Published var remotes: [RcloneRemote] = []
    @Published var providers: [ConfigProvider] = []
    @Published var transfers: [TransferSnapshot] = []
    @Published var activities: [ActivitySnapshot] = []
    @Published var tasks: [SavedTask] = []
    @Published var selectedSection: SidebarSection = .workspace
    @Published var activePane: PaneID = .primary
    @Published var isBootstrapping = true
    @Published var globalError: String?
    @Published var notice: String?
    @Published var showAddLocation = false
    @Published var showTaskEditor = false
    @Published var editingTask: SavedTask?
    @Published var showMount = false
    @Published var showReconfigure = false
    @Published var reconfiguringRemote: RcloneRemote?
    @Published var transferShelfExpanded = false
    @Published var showTextPreview = false
    @Published var textPreviewTitle = ""
    @Published var textPreview = ""
    @Published var showRcloneUpdate = false
    @Published var rcloneUpdateInfo: RcloneUpdateInfo?
    @Published var showQuitConfirmation = false
    @Published var mountSource = ""
    @Published var mountPresetArguments: [String] = []

    let primary: PaneState
    let secondary: PaneState
    private var pollTimer: Timer?
    private var hasStarted = false
    private var hasBootstrapped = false
    private var isPolling = false
    var permitsTermination = false

    init() {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        primary = PaneState(id: .primary, remote: "__local__", path: home)
        secondary = PaneState(id: .secondary, remote: "__local__", path: home)
    }

    var currentPane: PaneState { activePane == .primary ? primary : secondary }
    var otherPane: PaneState { activePane == .primary ? secondary : primary }
    var runningCount: Int { transfers.filter(\.status.isRunning).count + activities.filter(\.status.isRunning).count }

    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("--open-settings") { selectedSection = .settings }
        Task {
            await bootstrap()
            if arguments.contains("--open-add-location") {
                await loadProviders()
                showAddLocation = true
            }
            if arguments.contains("--open-rclone-update") {
                await checkRcloneUpdate()
            }
            await runScheduledUpdateChecks()
        }
        pollTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in await self?.pollWork() }
        }
    }

    func stop() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    func bootstrap() async {
        isBootstrapping = true
        do {
            let value: Bootstrap = try await background { try RustBridge.call("bootstrap", payload: EmptyPayload()) }
            settings = value.settings
            appVersion = value.appVersion
            rclone = value.rclone
            remotes = value.remotes
            transfers = value.transfers
            activities = value.activities
            tasks = value.tasks
            if !hasBootstrapped, let firstRemote = remotes.first(where: { !$0.isLocal }) {
                secondary.navigate(remote: firstRemote.name, path: "")
            }
            hasBootstrapped = true
            repairInvalidPaneLocations()
            await refresh(primary)
            await refresh(secondary)
        } catch {
            globalError = error.localizedDescription
        }
        isBootstrapping = false
    }

    func refresh(_ pane: PaneState, useCache: Bool = false) async {
        let token = UUID()
        pane.requestToken = token
        if useCache, let entries = pane.cachedEntries() {
            pane.entries = entries
            pane.selectedIDs.formIntersection(Set(entries.map(\.id)))
            pane.isLoading = false
            pane.error = nil
            return
        }
        pane.isLoading = true
        pane.error = nil
        let payload = BrowserPayload(remote: pane.remote, path: pane.path, sharedWithMe: pane.sharedWithMe)
        do {
            let entries: [BrowserEntry] = try await background { try RustBridge.call("listEntries", payload: payload) }
            guard pane.requestToken == token else { return }
            if pane.entries != entries { pane.entries = entries }
            pane.cache(entries)
            pane.selectedIDs.formIntersection(Set(entries.map(\.id)))
        } catch {
            guard pane.requestToken == token else { return }
            pane.error = error.localizedDescription
            pane.entries = []
        }
        if pane.requestToken == token { pane.isLoading = false }
    }

    func navigate(_ pane: PaneState, to entry: BrowserEntry) {
        guard entry.isDir else { return }
        pane.navigate(remote: pane.remote, path: entry.path)
        Task { await refresh(pane, useCache: true) }
    }

    func chooseRemote(_ remote: RcloneRemote, pane: PaneState) {
        let path = remote.isLocal ? FileManager.default.homeDirectoryForCurrentUser.path : ""
        pane.navigate(remote: remote.name, path: path)
        selectedSection = .workspace
        activePane = pane.id
        Task { await refresh(pane, useCache: true) }
    }

    func goUp(_ pane: PaneState) {
        let path: String
        if pane.remote == "__local__" {
            let current = URL(fileURLWithPath: pane.path)
            path = current.deletingLastPathComponent().path
        } else {
            path = pane.path.split(separator: "/").dropLast().joined(separator: "/")
        }
        guard path != pane.path else { return }
        pane.navigate(remote: pane.remote, path: path)
        Task { await refresh(pane, useCache: true) }
    }

    func goBack(_ pane: PaneState) {
        guard pane.goBack() != nil else { return }
        Task { await refresh(pane, useCache: true) }
    }

    func goForward(_ pane: PaneState) {
        guard pane.goForward() != nil else { return }
        Task { await refresh(pane, useCache: true) }
    }

    func addTab(_ pane: PaneState) {
        pane.newTab()
        Task { await refresh(pane, useCache: true) }
    }

    func selectTab(_ id: UUID, pane: PaneState) {
        pane.selectTab(id)
        Task { await refresh(pane, useCache: true) }
    }

    func closeTab(_ id: UUID, pane: PaneState) {
        pane.closeTab(id)
        Task { await refresh(pane, useCache: true) }
    }

    func toggleSharedWithMe(_ pane: PaneState) {
        pane.toggleSharedWithMe()
        Task { await refresh(pane, useCache: true) }
    }

    func createFolder(name: String, in pane: PaneState) async {
        let path = joinPath(pane.path, name, local: pane.remote == "__local__")
        do {
            try await backgroundVoid { try RustBridge.callVoid("createFolder", payload: PathPayload(remote: pane.remote, path: path, sharedWithMe: pane.sharedWithMe)) }
            await refresh(pane)
        } catch { globalError = error.localizedDescription }
    }

    func rename(_ entry: BrowserEntry, to name: String, in pane: PaneState) async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("renameEntry", payload: RenamePayload(remote: pane.remote, path: entry.path, newName: name, sharedWithMe: pane.sharedWithMe)) }
            await refresh(pane)
        } catch { globalError = error.localizedDescription }
    }

    func move(_ entry: BrowserEntry, to destination: String, in pane: PaneState) async {
        let clean = destination.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        do {
            try await backgroundVoid {
                try RustBridge.callVoid("moveEntry", payload: MovePayload(
                    remote: pane.remote,
                    source: entry.path,
                    destination: clean,
                    sharedWithMe: pane.sharedWithMe
                ))
            }
            await refresh(pane)
        } catch { globalError = error.localizedDescription }
    }

    func delete(_ entries: [BrowserEntry], in pane: PaneState) async {
        do {
            for entry in entries {
                try await backgroundVoid { try RustBridge.callVoid("deleteEntry", payload: DeletePayload(remote: pane.remote, path: entry.path, isDir: entry.isDir, sharedWithMe: pane.sharedWithMe)) }
            }
            await refresh(pane)
        } catch { globalError = error.localizedDescription }
    }

    func transferSelection(_ operation: TransferOperation) async {
        let sourcePane = currentPane
        let destinationPane = otherPane
        let selected = sourcePane.selectedEntries
        guard !selected.isEmpty else {
            notice = "Select one or more files first."
            return
        }
        do {
            for entry in selected {
                let request = TransferRequest(
                    direction: .copy,
                    operation: operation,
                    source: endpoint(remote: sourcePane.remote, path: entry.path),
                    destination: endpoint(remote: destinationPane.remote, path: joinPath(destinationPane.path, entry.name, local: destinationPane.remote == "__local__")),
                    isDirectory: entry.isDir,
                    extraArgs: sourcePane.sharedWithMe ? ["--drive-shared-with-me"] : [],
                    label: "\(operation.title) \(entry.name)"
                )
                let snapshot: TransferSnapshot = try await background { try RustBridge.call("startTransfer", payload: request) }
                transfers.insert(snapshot, at: 0)
            }
            transferShelfExpanded = true
        } catch { globalError = error.localizedDescription }
    }

    func startDownload(entries: [BrowserEntry], from pane: PaneState, to directory: String) async {
        await startExternalTransfer(entries: entries, pane: pane, directory: directory, direction: .download)
    }

    func startUpload(urls: [URL], to pane: PaneState) async {
        do {
            for url in urls {
                var isDirectory: ObjCBool = false
                _ = FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
                let request = TransferRequest(
                    direction: .upload, operation: .copy, source: url.path,
                    destination: endpoint(remote: pane.remote, path: joinPath(pane.path, url.lastPathComponent, local: pane.remote == "__local__")),
                    isDirectory: isDirectory.boolValue, extraArgs: settings.defaultUploadArgs,
                    label: "Upload \(url.lastPathComponent)"
                )
                let snapshot: TransferSnapshot = try await background { try RustBridge.call("startTransfer", payload: request) }
                transfers.insert(snapshot, at: 0)
            }
            transferShelfExpanded = true
        } catch { globalError = error.localizedDescription }
    }

    private func startExternalTransfer(entries: [BrowserEntry], pane: PaneState, directory: String, direction: TransferDirection) async {
        do {
            for entry in entries {
                let request = TransferRequest(
                    direction: direction, operation: .copy,
                    source: endpoint(remote: pane.remote, path: entry.path),
                    destination: URL(fileURLWithPath: directory).appendingPathComponent(entry.name).path,
                    isDirectory: entry.isDir, extraArgs: settings.defaultDownloadArgs + (pane.sharedWithMe ? ["--drive-shared-with-me"] : []),
                    label: "Download \(entry.name)"
                )
                let snapshot: TransferSnapshot = try await background { try RustBridge.call("startTransfer", payload: request) }
                transfers.insert(snapshot, at: 0)
            }
            transferShelfExpanded = true
        } catch { globalError = error.localizedDescription }
    }

    func stream(_ entry: BrowserEntry, in pane: PaneState) async {
        guard !entry.isDir else { return }
        let source = endpoint(remote: pane.remote, path: entry.path)
        do {
            let snapshot: ActivitySnapshot = try await background {
                try RustBridge.call("startStream", payload: StreamPayload(source: source))
            }
            activities.insert(snapshot, at: 0)
            selectedSection = .activity
        } catch { globalError = error.localizedDescription }
    }

    func startMount(source: String, destination: String, extraArgs: [String]) async {
        do {
            let snapshot: ActivitySnapshot = try await background { try RustBridge.call("startMount", payload: MountPayload(source: source, destination: destination, extraArgs: extraArgs)) }
            activities.insert(snapshot, at: 0)
            showMount = false
            selectedSection = .activity
        } catch { globalError = error.localizedDescription }
    }

    func presentMount(source: String? = nil, sharedWithMe: Bool? = nil) {
        let pane = currentPane
        mountSource = source ?? endpoint(remote: pane.remote, path: pane.path)
        mountPresetArguments = (sharedWithMe ?? pane.sharedWithMe) ? ["--drive-shared-with-me"] : []
        showMount = true
    }

    func cancelTransfer(_ id: String) async {
        do { try await backgroundVoid { try RustBridge.callVoid("cancelTransfer", payload: IDPayload(id: id)) }; await pollWork() }
        catch { globalError = error.localizedDescription }
    }

    func cancelActivity(_ id: String) async {
        do { try await backgroundVoid { try RustBridge.callVoid("cancelActivity", payload: IDPayload(id: id)) }; await pollWork() }
        catch { globalError = error.localizedDescription }
    }

    func clearFinishedWork() async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("clearFinishedWork", payload: EmptyPayload()) }
            transfers.removeAll { !$0.status.isRunning }
            activities.removeAll { !$0.status.isRunning }
        }
        catch { globalError = error.localizedDescription }
    }

    func copyCommand(for transfer: TransferSnapshot) async {
        let request = TransferRequest(
            direction: transfer.direction,
            operation: transfer.operation,
            source: transfer.source,
            destination: transfer.destination,
            isDirectory: transfer.isDirectory,
            extraArgs: transfer.extraArgs,
            label: transfer.label
        )
        await copyCommand(command: "copyCommand", payload: request)
    }

    func copyCommand(for task: SavedTask, dryRun: Bool = false) async {
        await copyCommand(
            command: "taskCommand",
            payload: StartTaskPayload(task: task, dryRun: dryRun)
        )
    }

    private func copyCommand<Payload: Encodable>(command: String, payload: Payload) async {
        do {
            let value: String = try await background {
                try RustBridge.call(command, payload: payload)
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            notice = "rclone command copied."
        } catch { globalError = error.localizedDescription }
    }

    func loadProviders() async {
        guard providers.isEmpty else { return }
        do { providers = try await background { try RustBridge.call("listProviders", payload: EmptyPayload()) } }
        catch { globalError = error.localizedDescription }
    }

    func completeLocation() async {
        do {
            remotes = try await background { try RustBridge.call("listRemotes", payload: EmptyPayload()) }
            showAddLocation = false
        } catch { globalError = error.localizedDescription }
    }

    func deleteRemote(_ remote: RcloneRemote) async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("deleteRemote", payload: NamePayload(name: remote.name)) }
            remotes = try await background { try RustBridge.call("listRemotes", payload: EmptyPayload()) }
            repairInvalidPaneLocations()
            await refresh(primary)
            await refresh(secondary)
        } catch { globalError = error.localizedDescription }
    }

    func reconfigure(_ remote: RcloneRemote) {
        reconfiguringRemote = remote
        showReconfigure = true
    }

    func completeReconfiguration() async {
        do {
            remotes = try await background { try RustBridge.call("listRemotes", payload: EmptyPayload()) }
            showReconfigure = false
            reconfiguringRemote = nil
            await refresh(primary)
            await refresh(secondary)
        } catch { globalError = error.localizedDescription }
    }

    func saveSettings(_ value: AppSettings) async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("saveSettings", payload: value) }
            let bootstrap: Bootstrap = try await background { try RustBridge.call("bootstrap", payload: EmptyPayload()) }
            settings = bootstrap.settings
            appVersion = bootstrap.appVersion
            rclone = bootstrap.rclone
            remotes = bootstrap.remotes
            if transfers != bootstrap.transfers { transfers = bootstrap.transfers }
            if activities != bootstrap.activities { activities = bootstrap.activities }
            if tasks != bootstrap.tasks { tasks = bootstrap.tasks }
            repairInvalidPaneLocations()
            primary.clearCache()
            secondary.clearCache()
            notice = "Settings saved."
            await refresh(primary)
            await refresh(secondary)
        } catch { globalError = error.localizedDescription }
    }

    func saveTask(_ task: SavedTask) async {
        do {
            let saved: SavedTask = try await background { try RustBridge.call("saveTask", payload: task) }
            if let index = tasks.firstIndex(where: { $0.id == saved.id }) { tasks[index] = saved } else { tasks.append(saved) }
            showTaskEditor = false
            editingTask = nil
        } catch { globalError = error.localizedDescription }
    }

    func deleteTask(_ id: String) async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("deleteTask", payload: IDPayload(id: id)) }
            tasks.removeAll { $0.id == id }
        } catch { globalError = error.localizedDescription }
    }

    func runTask(_ id: String, dryRun: Bool = false) async {
        do {
            let snapshot: TransferSnapshot = try await background { try RustBridge.call("runTask", payload: RunTaskPayload(id: id, dryRun: dryRun)) }
            transfers.insert(snapshot, at: 0)
            transferShelfExpanded = true
        } catch { globalError = error.localizedDescription }
    }

    func runTaskDraft(_ task: SavedTask, dryRun: Bool = false) async {
        do {
            let snapshot: TransferSnapshot = try await background {
                try RustBridge.call("startTask", payload: StartTaskPayload(task: task, dryRun: dryRun))
            }
            transfers.insert(snapshot, at: 0)
            transferShelfExpanded = true
            showTaskEditor = false
            editingTask = nil
        } catch { globalError = error.localizedDescription }
    }

    func presentTransferOptions(for entry: BrowserEntry, in pane: PaneState) {
        activePane = pane.id
        var task = SavedTask.blank(
            source: endpoint(remote: pane.remote, path: entry.path),
            destination: endpoint(
                remote: otherPane.remote,
                path: joinPath(otherPane.path, entry.name, local: otherPane.remote == "__local__")
            )
        )
        task.description = entry.name
        task.direction = .copy
        task.isDirectory = entry.isDir
        task.sharedWithMe = pane.sharedWithMe
        editingTask = task
        showTaskEditor = true
    }

    func publicLink(for entry: BrowserEntry, in pane: PaneState) async {
        do {
            let link: String = try await background { try RustBridge.call("publicLink", payload: BrowserPayload(remote: pane.remote, path: entry.path, sharedWithMe: pane.sharedWithMe)) }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(link, forType: .string)
            notice = "Public link copied."
        } catch { globalError = error.localizedDescription }
    }

    func size(of entry: BrowserEntry, in pane: PaneState) async {
        do {
            let summary: DirectorySummary = try await background { try RustBridge.call("directorySize", payload: BrowserPayload(remote: pane.remote, path: entry.path, sharedWithMe: pane.sharedWithMe)) }
            notice = "\(summary.count.formatted()) files · \(ByteCountFormatter.string(fromByteCount: Int64(summary.bytes), countStyle: .file))"
        } catch { globalError = error.localizedDescription }
    }

    func showTree(for entry: BrowserEntry, in pane: PaneState) async {
        do {
            let tree: String = try await background { try RustBridge.call("directoryTree", payload: BrowserPayload(remote: pane.remote, path: entry.path, sharedWithMe: pane.sharedWithMe)) }
            textPreviewTitle = "Directory Tree — \(entry.name)"
            textPreview = tree
            showTextPreview = true
        } catch { globalError = error.localizedDescription }
    }

    func export(_ entry: BrowserEntry, in pane: PaneState, destination: String, format: String) async {
        do {
            let count: UInt64 = try await background {
                try RustBridge.call("exportListing", payload: ExportPayload(remote: pane.remote, path: entry.path, destination: destination, format: format, sharedWithMe: pane.sharedWithMe))
            }
            notice = "Exported \(count.formatted()) items."
        } catch { globalError = error.localizedDescription }
    }

    func setConfigPassword(_ password: String) async {
        do {
            try await backgroundVoid { try RustBridge.callVoid("setPassword", payload: PasswordPayload(password: password)) }
            notice = password.isEmpty ? "Configuration password cleared." : "Configuration unlocked for this session."
            await bootstrap()
        } catch { globalError = error.localizedDescription }
    }

    func revealConfigFile() async {
        do {
            let path: String = try await background { try RustBridge.call("configFile", payload: EmptyPayload()) }
            NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
        } catch { globalError = error.localizedDescription }
    }

    func checkRcloneUpdate(showResult: Bool = true) async {
        do {
            let result: RcloneUpdateInfo = try await background {
                try RustBridge.call("checkRcloneUpdate", payload: EmptyPayload())
            }
            if showResult {
                rcloneUpdateInfo = result
                showRcloneUpdate = true
            } else if let stable = result.stable,
                      compareVersions(stable.version, result.currentVersion) == .orderedDescending {
                notice = "rclone \(stable.version) is available."
            }
        } catch {
            if showResult { globalError = error.localizedDescription }
        }
    }

    func checkAppUpdate(showResult: Bool = true) async {
        do {
            var request = URLRequest(url: URL(string: "https://api.github.com/repos/kapitainsky/RcloneBrowser/releases/latest")!)
            request.setValue("Rclone-Browser-Native/\(appVersion)", forHTTPHeaderField: "User-Agent")
            request.timeoutInterval = 15
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
                throw UpdateCheckError.serviceUnavailable
            }
            let release = try JSONDecoder().decode(GitHubRelease.self, from: data)
            let available = compareVersions(release.tagName, appVersion) == .orderedDescending
            let result = available
                ? "Rclone Browser \(release.tagName) is available.\n\n\(release.htmlURL)"
                : "Rclone Browser \(appVersion) is up to date."
            if showResult {
                textPreviewTitle = "Application Update Check"
                textPreview = result
                showTextPreview = true
            } else if available {
                notice = "Rclone Browser \(release.tagName) is available."
            }
        } catch {
            if showResult { globalError = "Could not check for application updates: \(error.localizedDescription)" }
        }
    }

    func pollWork() async {
        guard !isPolling, runningCount > 0 else { return }
        isPolling = true
        defer { isPolling = false }
        do {
            let previous = Dictionary(uniqueKeysWithValues: transfers.map { ($0.id, $0.status) })
            async let nextTransfers: [TransferSnapshot] = background { try RustBridge.call("listTransfers", payload: EmptyPayload()) }
            async let nextActivities: [ActivitySnapshot] = background { try RustBridge.call("listActivities", payload: EmptyPayload()) }
            let transferValues = try await nextTransfers
            let activityValues = try await nextActivities
            if transfers != transferValues { transfers = transferValues }
            if activities != activityValues { activities = activityValues }
            for transfer in transfers where previous[transfer.id]?.isRunning == true && !transfer.status.isRunning {
                notifyCompletion(transfer)
            }
        } catch {
            // Polling is deliberately quiet; direct actions surface errors.
        }
    }

    func requestQuit() {
        if runningCount > 0 { showQuitConfirmation = true }
        else {
            permitsTermination = true
            NSApplication.shared.terminate(nil)
        }
    }

    func quitAndCancelWork() async {
        do { try await backgroundVoid { try RustBridge.callVoid("cancelAll", payload: EmptyPayload()) } }
        catch { globalError = error.localizedDescription; return }
        permitsTermination = true
        NSApplication.shared.terminate(nil)
    }

    private func notifyCompletion(_ transfer: TransferSnapshot) {
        guard settings.notifyFinishedTransfers else { return }
        Task {
            let center = UNUserNotificationCenter.current()
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            let content = UNMutableNotificationContent()
            content.title = transfer.status == .completed ? "Transfer Complete" : "Transfer \(transfer.status.rawValue.capitalized)"
            content.body = transfer.label ?? URL(fileURLWithPath: transfer.source).lastPathComponent
            if transfer.status == .completed { content.sound = .default }
            try? await center.add(UNNotificationRequest(identifier: transfer.id, content: content, trigger: nil))
        }
    }

    func endpoint(remote: String, path: String) -> String {
        remote == "__local__" ? path : "\(remote):\(path.trimmingCharacters(in: CharacterSet(charactersIn: "/")))"
    }

    func joinPath(_ base: String, _ name: String, local: Bool) -> String {
        if local { return URL(fileURLWithPath: base, isDirectory: true).appendingPathComponent(name).path }
        let cleanBase = base.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let cleanName = name.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return cleanBase.isEmpty ? cleanName : "\(cleanBase)/\(cleanName)"
    }

    private func repairInvalidPaneLocations() {
        let available = Set(remotes.map(\.name))
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        for pane in [primary, secondary] where !available.contains(pane.remote) {
            pane.navigate(remote: "__local__", path: home)
        }
    }

    private func runScheduledUpdateChecks() async {
        let defaults = UserDefaults.standard
        let today = String(ISO8601DateFormatter().string(from: Date()).prefix(10))
        if settings.checkRcloneUpdates,
           (defaults.string(forKey: "RcloneBrowser.lastRcloneUpdateCheck") ?? "") != today {
            await checkRcloneUpdate(showResult: false)
            defaults.set(today, forKey: "RcloneBrowser.lastRcloneUpdateCheck")
        }
        if settings.checkAppUpdates,
           (defaults.string(forKey: "RcloneBrowser.lastAppUpdateCheck") ?? "") != today {
            await checkAppUpdate(showResult: false)
            defaults.set(today, forKey: "RcloneBrowser.lastAppUpdateCheck")
        }
    }

    private func compareVersions(_ left: String, _ right: String) -> ComparisonResult {
        let leftParts = left.trimmingCharacters(in: CharacterSet(charactersIn: "vV"))
            .split(whereSeparator: { !$0.isNumber }).compactMap { Int($0) }
        let rightParts = right.trimmingCharacters(in: CharacterSet(charactersIn: "vV"))
            .split(whereSeparator: { !$0.isNumber }).compactMap { Int($0) }
        for index in 0..<max(leftParts.count, rightParts.count) {
            let lhs = index < leftParts.count ? leftParts[index] : 0
            let rhs = index < rightParts.count ? rightParts[index] : 0
            if lhs != rhs { return lhs < rhs ? .orderedAscending : .orderedDescending }
        }
        return .orderedSame
    }

    private func background<Result>(_ work: @escaping () throws -> Result) async throws -> Result {
        try await Task.detached(priority: .userInitiated) { try work() }.value
    }

    private func backgroundVoid(_ work: @escaping () throws -> Void) async throws {
        try await Task.detached(priority: .userInitiated) { try work() }.value
    }
}

private struct GitHubRelease: Decodable {
    var tagName: String
    var htmlURL: String

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
    }
}

private enum UpdateCheckError: LocalizedError {
    case serviceUnavailable

    var errorDescription: String? { "The update service did not return a successful response." }
}
