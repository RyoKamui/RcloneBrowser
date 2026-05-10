import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct WorkspaceView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            workspaceToolbar
            Divider().opacity(0.7)
            HStack(spacing: 0) {
                FilePaneView(pane: model.primary)
                if model.settings.dualPane {
                    Divider()
                    FilePaneView(pane: model.secondary)
                }
            }
            if model.settings.showTransferShelf && (!model.transfers.isEmpty || model.transferShelfExpanded) {
                Divider().opacity(0.7)
                TransferShelf()
            }
        }
        .background(AppDesign.appSurface)
    }

    private var workspaceToolbar: some View {
        HStack(spacing: 7) {
            ToolbarIconButton(symbol: "chevron.left", help: "Back", disabled: !model.currentPane.canGoBack) {
                model.goBack(model.currentPane)
            }
            ToolbarIconButton(symbol: "chevron.right", help: "Forward", disabled: !model.currentPane.canGoForward) {
                model.goForward(model.currentPane)
            }
            ToolbarIconButton(symbol: "arrow.up", help: "Parent folder") { model.goUp(model.currentPane) }
            ToolbarIconButton(symbol: "arrow.clockwise", help: "Refresh") { Task { await model.refresh(model.currentPane) } }

            Divider().frame(height: 18).padding(.horizontal, 2)

            breadcrumb

            Spacer(minLength: 12)

            Button {
                Task { await model.transferSelection(.copy) }
            } label: {
                Label(model.activePane == .primary ? "Copy →" : "← Copy", systemImage: "doc.on.doc")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(model.currentPane.selectedIDs.isEmpty || !model.settings.dualPane)
            .help("Copy selection to the other pane")

            Button {
                Task { await model.transferSelection(.move) }
            } label: {
                Label(model.activePane == .primary ? "Move →" : "← Move", systemImage: "arrow.right")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(model.currentPane.selectedIDs.isEmpty || !model.settings.dualPane)
            .help("Move selection to the other pane")

            ToolbarIconButton(
                symbol: "slider.horizontal.3",
                help: "Transfer options",
                disabled: model.currentPane.selectedEntries.count != 1 || !model.settings.dualPane
            ) {
                if let entry = model.currentPane.selectedEntries.first {
                    model.presentTransferOptions(for: entry, in: model.currentPane)
                }
            }

            Divider().frame(height: 18).padding(.horizontal, 2)

            ToolbarIconButton(symbol: "plus", help: "New folder") {
                NotificationCenter.default.post(name: .newFolderRequested, object: nil)
            }
            .disabled(model.currentPane.sharedWithMe)
            ToolbarIconButton(symbol: "square.and.arrow.up", help: "Upload", disabled: model.currentPane.sharedWithMe) { chooseUpload() }
            ToolbarIconButton(symbol: "square.and.arrow.down", help: "Download", disabled: model.currentPane.selectedIDs.isEmpty) { chooseDownload() }
            ToolbarIconButton(symbol: model.settings.dualPane ? "rectangle" : "rectangle.split.2x1", help: model.settings.dualPane ? "Use one pane" : "Use two panes") {
                var settings = model.settings
                settings.dualPane.toggle()
                Task { await model.saveSettings(settings) }
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 48)
        .background(.bar)
    }

    private var breadcrumb: some View {
        HStack(spacing: 6) {
            Image(systemName: model.remotes.first(where: { $0.name == model.currentPane.remote })?.symbol ?? "network")
                .foregroundStyle(.secondary)
            Text(model.remotes.first(where: { $0.name == model.currentPane.remote })?.displayName ?? model.currentPane.remote)
                .fontWeight(.medium)
            if !model.currentPane.path.isEmpty {
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                Text(model.currentPane.path)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.head)
            }
        }
        .font(.system(size: 12))
        .padding(.horizontal, 10)
        .frame(height: 28)
        .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private func chooseUpload() {
        let panel = NSOpenPanel()
        panel.title = "Choose files or folders to upload"
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        if let path = model.settings.defaultUploadDir { panel.directoryURL = URL(fileURLWithPath: path) }
        guard panel.runModal() == .OK else { return }
        Task { await model.startUpload(urls: panel.urls, to: model.currentPane) }
    }

    private func chooseDownload() {
        let panel = NSOpenPanel()
        panel.title = "Choose a download folder"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if let path = model.settings.defaultDownloadDir { panel.directoryURL = URL(fileURLWithPath: path) }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await model.startDownload(entries: model.currentPane.selectedEntries, from: model.currentPane, to: url.path) }
    }
}

private struct NameEditorItem: Identifiable {
    enum Kind { case folder, rename(BrowserEntry), move(BrowserEntry) }
    let id = UUID()
    let kind: Kind
    var initialValue: String
}

struct FilePaneView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var pane: PaneState
    @State private var editor: NameEditorItem?
    @State private var deleteCandidates: [BrowserEntry] = []
    @State private var showingDeleteConfirmation = false

    var body: some View {
        VStack(spacing: 0) {
            tabBar
            paneHeader
            fileList
        }
        .background(AppDesign.appSurface)
        .onReceive(NotificationCenter.default.publisher(for: .newFolderRequested)) { _ in
            guard model.currentPane.id == pane.id, !pane.sharedWithMe else { return }
            editor = NameEditorItem(kind: .folder, initialValue: "")
        }
        .sheet(item: $editor) { item in
            NameEditorSheet(item: item) { value in
                switch item.kind {
                case .folder: Task { await model.createFolder(name: value, in: pane) }
                case .rename(let entry): Task { await model.rename(entry, to: value, in: pane) }
                case .move(let entry): Task { await model.move(entry, to: value, in: pane) }
                }
            }
        }
        .confirmationDialog("Delete selected items?", isPresented: $showingDeleteConfirmation) {
            Button("Delete \(deleteCandidates.count) item\(deleteCandidates.count == 1 ? "" : "s")", role: .destructive) {
                Task { await model.delete(deleteCandidates, in: pane) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This asks rclone to permanently remove the selection.")
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard !urls.isEmpty, !pane.sharedWithMe else { return false }
            Task { await model.startUpload(urls: urls, to: pane) }
            return true
        }
    }

    private var tabBar: some View {
        HStack(spacing: 3) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 3) {
                    ForEach(pane.tabs) { tab in
                        HStack(spacing: 6) {
                            Button {
                                model.activePane = pane.id
                                model.selectTab(tab.id, pane: pane)
                            } label: {
                                HStack(spacing: 5) {
                                    Image(systemName: "folder").font(.caption2)
                                    Text(tab.title).lineLimit(1)
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            if pane.tabs.count > 1 && pane.activeTabID == tab.id {
                                Button { model.closeTab(tab.id, pane: pane) } label: {
                                    Image(systemName: "xmark").font(.system(size: 8, weight: .semibold))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(pane.activeTabID == tab.id ? Color.primary : Color.secondary)
                        .padding(.horizontal, 9)
                        .frame(height: 27)
                        .background(pane.activeTabID == tab.id ? Color.primary.opacity(0.075) : Color.clear, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    }
                }
            }
            Button { model.addTab(pane) } label: {
                Image(systemName: "plus").font(.system(size: 10, weight: .semibold)).frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 9)
        .frame(height: 34)
        .background(Color.primary.opacity(0.025))
    }

    private var paneHeader: some View {
        HStack(spacing: 8) {
            Menu {
                ForEach(model.remotes) { remote in
                    Button { model.chooseRemote(remote, pane: pane) } label: {
                        Label(remote.displayName, systemImage: remote.symbol)
                    }
                }
                if model.remotes.first(where: { $0.name == pane.remote })?.type == "drive" {
                    Divider()
                    Button { model.toggleSharedWithMe(pane) } label: {
                        Label("Shared with me", systemImage: pane.sharedWithMe ? "checkmark.circle.fill" : "person.2")
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: model.remotes.first(where: { $0.name == pane.remote })?.symbol ?? "network")
                    Text(model.remotes.first(where: { $0.name == pane.remote })?.displayName ?? pane.remote)
                    Image(systemName: "chevron.down").font(.caption2).foregroundStyle(.secondary)
                }
                .font(.system(size: 12, weight: .medium))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()

            Text(pane.path.isEmpty ? "/" : pane.path)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.head)

            Spacer()

            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Filter", text: $pane.search)
                    .textFieldStyle(.plain)
                    .frame(width: 110)
                if !pane.search.isEmpty {
                    Button { pane.search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary) }
                        .buttonStyle(.plain)
                }
            }
            .font(.system(size: 11))
            .padding(.horizontal, 7)
            .frame(height: 24)
            .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 7, style: .continuous))

            Menu {
                ForEach(FileSort.allCases) { option in
                    Button {
                        if pane.sort == option { pane.sortAscending.toggle() }
                        else { pane.sort = option; pane.sortAscending = true }
                    } label: {
                        Label(option.title, systemImage: pane.sort == option ? (pane.sortAscending ? "arrow.up" : "arrow.down") : "circle")
                    }
                }
            } label: {
                Image(systemName: "arrow.up.arrow.down").font(.system(size: 11))
            }
            .menuStyle(.borderlessButton).fixedSize().help("Sort files")
        }
        .padding(.horizontal, 10)
        .frame(height: 38)
    }

    private var fileList: some View {
        NativeFileTable(
            entries: pane.visibleEntries,
            selectedIDs: $pane.selectedIDs,
            compactRows: model.settings.compactRows,
            showFolderIcons: model.settings.showFolderIcons,
            showFileIcons: model.settings.showFileIcons,
            alternatingRows: model.settings.alternatingRows,
            iconSize: model.settings.iconSize,
            sort: pane.sort,
            sortAscending: pane.sortAscending,
            isLocal: pane.remote == "__local__",
            isReadOnly: pane.sharedWithMe,
            onFocus: {
                if model.activePane != pane.id { model.activePane = pane.id }
            },
            onOpen: open,
            onDelete: { requestDelete(pane.selectedEntries) },
            onSort: { option, ascending in
                model.activePane = pane.id
                pane.sort = option
                pane.sortAscending = ascending
            },
            onAction: performTableAction,
            onDrop: { urls in
                guard !urls.isEmpty, !pane.sharedWithMe else { return false }
                Task { await model.startUpload(urls: urls, to: pane) }
                return true
            }
        )
        .background(AppDesign.appSurface)
        .overlay {
            if pane.isLoading {
                ProgressView().controlSize(.small)
            } else if let error = pane.error {
                ContentUnavailableView("Couldn’t Open This Location", systemImage: "exclamationmark.triangle", description: Text(error))
                    .contentShape(Rectangle())
                    .onTapGesture { model.activePane = pane.id }
            } else if pane.visibleEntries.isEmpty {
                ContentUnavailableView(pane.search.isEmpty ? "Empty Folder" : "No Matches", systemImage: pane.search.isEmpty ? "folder" : "magnifyingglass")
                    .contentShape(Rectangle())
                    .onTapGesture { model.activePane = pane.id }
            }
        }
    }

    private func performTableAction(_ entry: BrowserEntry, _ action: FileTableAction) {
        switch action {
        case .open:
            open(entry)
        case .copyToOtherPane:
            select(entry)
            Task { await model.transferSelection(.copy) }
        case .moveToOtherPane:
            select(entry)
            Task { await model.transferSelection(.move) }
        case .transferOptions:
            model.presentTransferOptions(for: entry, in: pane)
        case .download:
            select(entry)
            chooseDownload()
        case .copyPath:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.endpoint(remote: pane.remote, path: entry.path), forType: .string)
            model.notice = "rclone path copied."
        case .rename:
            editor = NameEditorItem(kind: .rename(entry), initialValue: entry.name)
        case .moveWithinLocation:
            editor = NameEditorItem(kind: .move(entry), initialValue: entry.path)
        case .publicLink:
            Task { await model.publicLink(for: entry, in: pane) }
        case .calculateSize:
            Task { await model.size(of: entry, in: pane) }
        case .showTree:
            Task { await model.showTree(for: entry, in: pane) }
        case .exportListing:
            chooseExport(entry)
        case .mount:
            model.activePane = pane.id
            model.presentMount(
                source: model.endpoint(remote: pane.remote, path: entry.path),
                sharedWithMe: pane.sharedWithMe
            )
        case .stream:
            Task { await model.stream(entry, in: pane) }
        case .delete:
            select(entry)
            requestDelete(pane.selectedEntries)
        }
    }

    private func select(_ entry: BrowserEntry) {
        model.activePane = pane.id
        if !pane.selectedIDs.contains(entry.id) { pane.selectedIDs = Set([entry.id]) }
    }

    private func open(_ entry: BrowserEntry) {
        model.activePane = pane.id
        if entry.isDir {
            model.navigate(pane, to: entry)
        } else if pane.remote == "__local__" {
            NSWorkspace.shared.open(URL(fileURLWithPath: entry.path))
        } else {
            Task { await model.stream(entry, in: pane) }
        }
    }

    private func chooseDownload() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if let path = model.settings.defaultDownloadDir { panel.directoryURL = URL(fileURLWithPath: path) }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await model.startDownload(entries: pane.selectedEntries, from: pane, to: url.path) }
    }

    private func chooseExport(_ entry: BrowserEntry) {
        let panel = NSSavePanel()
        panel.title = "Export Directory Listing"
        panel.nameFieldStringValue = "\(entry.name)-listing.csv"
        panel.allowedContentTypes = [.commaSeparatedText, .plainText]
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let format = url.pathExtension.lowercased() == "txt" ? "txt" : "csv"
        Task { await model.export(entry, in: pane, destination: url.path, format: format) }
    }

    private func requestDelete(_ entries: [BrowserEntry]) {
        guard !entries.isEmpty, !pane.sharedWithMe else { return }
        deleteCandidates = entries
        if model.settings.confirmDelete { showingDeleteConfirmation = true }
        else { Task { await model.delete(entries, in: pane) } }
    }
}

private struct NameEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    let item: NameEditorItem
    let onSave: (String) -> Void
    @State private var value: String

    init(item: NameEditorItem, onSave: @escaping (String) -> Void) {
        self.item = item
        self.onSave = onSave
        _value = State(initialValue: item.initialValue)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            SectionHeading(title: title, detail: subtitle)
            TextField("Name", text: $value)
                .textFieldStyle(.roundedBorder)
                .onSubmit(save)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Save", action: save).keyboardShortcut(.defaultAction).disabled(value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 390)
    }

    private var title: String {
        switch item.kind {
        case .folder: return "New Folder"
        case .rename: return "Rename Item"
        case .move: return "Move Item"
        }
    }

    private var subtitle: String {
        switch item.kind {
        case .folder: return "Create it in the active location."
        case .rename: return "Enter a new name for this item."
        case .move: return "Enter a new full path within this location."
        }
    }

    private func save() {
        let clean = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        onSave(clean)
        dismiss()
    }
}

struct TransferShelf: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            Button {
                withAnimation(.easeInOut(duration: 0.18)) { model.transferShelfExpanded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "arrow.up.arrow.down.circle")
                    Text("Transfers").fontWeight(.medium)
                    if model.runningCount > 0 {
                        Text("\(model.runningCount) active").foregroundStyle(.secondary)
                    }
                    Spacer()
                    if let transfer = model.transfers.first(where: { $0.status.isRunning }) {
                        Text(transfer.label ?? transfer.source).foregroundStyle(.secondary).lineLimit(1)
                        if let fraction = transfer.fraction {
                            ProgressView(value: fraction).frame(width: 90)
                        } else {
                            ProgressView().controlSize(.mini)
                        }
                    }
                    Image(systemName: model.transferShelfExpanded ? "chevron.down" : "chevron.up").font(.caption)
                }
                .font(.system(size: 11))
                .padding(.horizontal, 12)
                .frame(height: 34)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if model.transferShelfExpanded {
                Divider().opacity(0.6)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(model.transfers.prefix(8)) { transfer in
                            CompactTransferCard(transfer: transfer)
                        }
                    }
                    .padding(10)
                }
                .frame(height: 92)
            }
        }
        .background(.bar)
    }
}

private struct CompactTransferCard: View {
    @EnvironmentObject private var model: AppModel
    let transfer: TransferSnapshot

    var body: some View {
        HStack(spacing: 10) {
            SymbolBadge(symbol: transfer.operation.symbol, tint: statusColor, size: 30)
            VStack(alignment: .leading, spacing: 5) {
                Text(transfer.label ?? URL(fileURLWithPath: transfer.source).lastPathComponent)
                    .font(.system(size: 11, weight: .medium)).lineLimit(1)
                if let fraction = transfer.fraction {
                    ProgressView(value: fraction).frame(width: 150)
                } else {
                    Text(transfer.status.rawValue.capitalized).font(.caption2).foregroundStyle(.secondary)
                }
            }
            if transfer.status.isRunning {
                Button { Task { await model.cancelTransfer(transfer.id) } } label: { Image(systemName: "xmark.circle") }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 10)
        .frame(width: 260, height: 62)
        .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var statusColor: Color {
        switch transfer.status {
        case .completed: return .green
        case .failed: return .red
        case .cancelled: return .secondary
        default: return .accentColor
        }
    }
}
