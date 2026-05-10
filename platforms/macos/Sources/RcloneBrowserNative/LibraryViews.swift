import AppKit
import SwiftUI

struct ActivityView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            pageHeader
            Divider().opacity(0.7)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    activitySection
                    transferSection
                }
                .padding(24)
                .frame(maxWidth: 920)
                .frame(maxWidth: .infinity)
            }
        }
    }

    private var pageHeader: some View {
        HStack {
            SectionHeading(title: "Activity", detail: "Transfers, mounts, and streams")
            Spacer()
            Button("Clear Finished") { Task { await model.clearFinishedWork() } }
                .buttonStyle(.bordered).controlSize(.small)
                .disabled(
                    !model.transfers.contains { !$0.status.isRunning }
                        && !model.activities.contains { !$0.status.isRunning }
                )
            Button { model.presentMount() } label: { Label("Mount", systemImage: "externaldrive.badge.plus") }
                .buttonStyle(.borderedProminent).controlSize(.small)
        }
        .padding(.horizontal, 24)
        .frame(height: 70)
    }

    @ViewBuilder
    private var activitySection: some View {
        if !model.activities.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text("MOUNTS & STREAMS").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                ForEach(model.activities) { activity in ActivityRow(activity: activity) }
            }
        }
    }

    private var transferSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("TRANSFERS").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
            if model.transfers.isEmpty {
                ContentUnavailableView("No Transfer Activity", systemImage: "arrow.up.arrow.down", description: Text("Copy, move, upload, and download activity appears here."))
                    .frame(maxWidth: .infinity, minHeight: 260)
                    .appCard()
            } else {
                ForEach(model.transfers) { transfer in TransferRow(transfer: transfer) }
            }
        }
    }
}

private struct TransferRow: View {
    @EnvironmentObject private var model: AppModel
    let transfer: TransferSnapshot
    @State private var showLog = false

    var body: some View {
        VStack(spacing: 11) {
            HStack(spacing: 12) {
                SymbolBadge(symbol: transfer.operation.symbol, tint: color, size: 36)
                VStack(alignment: .leading, spacing: 3) {
                    Text(transfer.label ?? transfer.operation.title).fontWeight(.medium).lineLimit(1)
                    Text("\(transfer.source)  →  \(transfer.destination)")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 3) {
                    Text(transfer.status.rawValue.capitalized).font(.caption.weight(.medium)).foregroundStyle(color)
                    Text(progressText).font(.caption2.monospacedDigit()).foregroundStyle(.secondary)
                }
                if transfer.status.isRunning {
                    Button("Cancel") { Task { await model.cancelTransfer(transfer.id) } }
                        .buttonStyle(.bordered).controlSize(.small)
                } else if !transfer.logTail.isEmpty {
                    Button { showLog.toggle() } label: { Image(systemName: "text.alignleft") }
                        .buttonStyle(.borderless).help("Show log")
                }
                Button { Task { await model.copyCommand(for: transfer) } } label: {
                    Image(systemName: "doc.on.clipboard")
                }
                .buttonStyle(.borderless).help("Copy rclone command")
            }
            if transfer.status.isRunning {
                if let fraction = transfer.fraction { ProgressView(value: fraction) }
                else { ProgressView() }
            }
            if showLog {
                Text(transfer.logTail.suffix(8).joined(separator: "\n"))
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(9)
                    .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 8))
            }
        }
        .appCard(padding: 13)
    }

    private var progressText: String {
        var values = [ByteCountFormatter.string(fromByteCount: Int64(transfer.bytes), countStyle: .file)]
        if let speed = transfer.speed { values.append("\(ByteCountFormatter.string(fromByteCount: Int64(speed), countStyle: .file))/s") }
        return values.joined(separator: " · ")
    }

    private var color: Color {
        switch transfer.status {
        case .completed: return .green
        case .failed: return .red
        case .cancelled: return .secondary
        default: return .accentColor
        }
    }
}

private struct ActivityRow: View {
    @EnvironmentObject private var model: AppModel
    let activity: ActivitySnapshot

    var body: some View {
        HStack(spacing: 12) {
            SymbolBadge(symbol: activity.kind == .mount ? "externaldrive" : "play.circle", tint: activity.status == .failed ? .red : .accentColor, size: 36)
            VStack(alignment: .leading, spacing: 3) {
                Text(activity.kind == .mount ? "Mounted Location" : "Streaming").fontWeight(.medium)
                Text(activity.source).font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            Text(activity.status.rawValue.capitalized).font(.caption).foregroundStyle(.secondary)
            if activity.status.isRunning {
                Button(activity.kind == .mount ? "Unmount" : "Stop") { Task { await model.cancelActivity(activity.id) } }
                    .buttonStyle(.bordered).controlSize(.small)
            }
        }
        .appCard(padding: 13)
    }
}

struct TasksView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                SectionHeading(title: "Saved Tasks", detail: "Reusable copy, move, and sync recipes")
                Spacer()
                Button {
                    model.editingTask = nil
                    model.showTaskEditor = true
                } label: { Label("New Task", systemImage: "plus") }
                    .buttonStyle(.borderedProminent).controlSize(.small)
            }
            .padding(.horizontal, 24).frame(height: 70)
            Divider().opacity(0.7)
            ScrollView {
                LazyVStack(spacing: 11) {
                    if model.tasks.isEmpty {
                        ContentUnavailableView("No Saved Tasks", systemImage: "clock.arrow.circlepath", description: Text("Save repeatable transfers and sync jobs here."))
                            .frame(maxWidth: .infinity, minHeight: 330)
                    } else {
                        ForEach(model.tasks) { task in TaskRow(task: task) }
                    }
                }
                .padding(24)
                .frame(maxWidth: 900)
                .frame(maxWidth: .infinity)
            }
        }
    }
}

private struct TaskRow: View {
    @EnvironmentObject private var model: AppModel
    let task: SavedTask

    var body: some View {
        HStack(spacing: 13) {
            SymbolBadge(symbol: task.operation.symbol, size: 38)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 7) {
                    Text(task.description).fontWeight(.medium)
                    Text(task.operation.title.uppercased())
                        .font(.system(size: 9, weight: .bold)).foregroundStyle(.secondary)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Color.primary.opacity(0.06), in: Capsule())
                }
                Text("\(task.source)  →  \(task.destination)")
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            Menu {
                Button("Dry Run") { Task { await model.runTask(task.id, dryRun: true) } }
                Button("Edit") { model.editingTask = task; model.showTaskEditor = true }
                Divider()
                Button("Delete", role: .destructive) { Task { await model.deleteTask(task.id) } }
            } label: { Image(systemName: "ellipsis.circle") }
                .menuStyle(.borderlessButton).fixedSize()
            Button("Run") { Task { await model.runTask(task.id) } }
                .buttonStyle(.borderedProminent).controlSize(.small)
        }
        .appCard(padding: 14)
    }
}

private enum SettingsCategory: String, CaseIterable, Identifiable {
    case general = "General"
    case transfers = "Transfers"
    case advanced = "Advanced"
    var id: String { rawValue }
}

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var draft = AppSettings.fallback
    @State private var category: SettingsCategory = .general
    @State private var configPassword = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                SectionHeading(title: "Settings", detail: "Core behavior first; specialist options stay out of the way")
                Spacer()
                Button("Revert") { draft = model.settings }.buttonStyle(.bordered).controlSize(.small)
                    .disabled(draft == model.settings)
                Button("Save") { Task { await model.saveSettings(draft) } }.buttonStyle(.borderedProminent).controlSize(.small)
                    .disabled(draft == model.settings)
            }
            .padding(.horizontal, 24).frame(height: 70)
            Divider().opacity(0.7)
            Picker("Category", selection: $category) {
                ForEach(SettingsCategory.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 440)
            .padding(.top, 18)
            ScrollView {
                VStack(spacing: 14) {
                    switch category {
                    case .general: generalSettings
                    case .transfers: transferSettings
                    case .advanced: advancedSettings
                    }
                }
                .padding(20)
                .frame(maxWidth: 760)
                .frame(maxWidth: .infinity)
            }
        }
        .onAppear { draft = model.settings }
    }

    private var generalSettings: some View {
        Group {
            settingsCard(title: "rclone", detail: "The engine behind every location and transfer", symbol: "bolt.horizontal.circle") {
                HStack(spacing: 9) {
                    Circle().fill(model.rclone.available ? Color.green : Color.orange).frame(width: 8, height: 8)
                    Text(model.rclone.available ? (model.rclone.version ?? "Connected") : (model.rclone.error ?? "Not available"))
                        .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    Spacer()
                }
                labeledField("Executable", text: $draft.rclonePath, button: "Choose…", action: chooseRclone)
                labeledOptionalField("Config file", text: $draft.configPath, button: "Choose…", action: chooseConfig)
                HStack {
                    SecureField("Encrypted config password", text: $configPassword).textFieldStyle(.roundedBorder)
                    Button("Unlock Session") { Task { await model.setConfigPassword(configPassword) } }.controlSize(.small)
                    Button("Reveal Config") { Task { await model.revealConfigFile() } }.controlSize(.small)
                    Button("Check Update") { Task { await model.checkRcloneUpdate() } }.controlSize(.small)
                }
            }
            settingsCard(title: "Appearance", detail: "Native, restrained, and consistent with macOS", symbol: "circle.lefthalf.filled") {
                settingRow("Theme", detail: "Follow macOS unless you need a fixed appearance") {
                    Picker("Theme", selection: $draft.theme) { ForEach(AppTheme.allCases) { Text($0.title).tag($0) } }
                        .labelsHidden().pickerStyle(.segmented).frame(width: 220)
                }
                Divider()
                Toggle("Use two file panes", isOn: $draft.dualPane)
                Toggle("Show transfer shelf", isOn: $draft.showTransferShelf)
                Toggle("Compact file rows", isOn: $draft.compactRows)
                Toggle("Show menu bar item", isOn: $draft.alwaysShowTray)
                Toggle("Keep running when the window closes", isOn: $draft.closeToTray)
            }
            settingsCard(title: "Browsing", detail: "Defaults used in every tab", symbol: "folder") {
                Toggle("Show hidden files", isOn: $draft.showHidden)
                Toggle("Show folder icons", isOn: $draft.showFolderIcons)
                Toggle("Show file icons", isOn: $draft.showFileIcons)
                Toggle("Alternate row backgrounds", isOn: $draft.alternatingRows)
                Toggle("Confirm before deleting", isOn: $draft.confirmDelete)
                settingRow("Icon size") {
                    Picker("Icon size", selection: $draft.iconSize) { ForEach(IconSize.allCases) { Text($0.title).tag($0) } }
                        .labelsHidden().frame(width: 130)
                }
            }
        }
    }

    private var transferSettings: some View {
        Group {
            settingsCard(title: "Default folders", detail: "Suggested locations for uploads and downloads", symbol: "arrow.up.arrow.down") {
                labeledOptionalField("Download to", text: $draft.defaultDownloadDir, button: "Choose…") { chooseDirectory(for: \AppSettings.defaultDownloadDir) }
                labeledOptionalField("Upload from", text: $draft.defaultUploadDir, button: "Choose…") { chooseDirectory(for: \AppSettings.defaultUploadDir) }
            }
            settingsCard(title: "Playback & mounts", detail: "Commands for remote media and mounted filesystems", symbol: "play.square.stack") {
                labeledField("Stream command", text: $draft.streamCommand)
                labeledField("Mount arguments", text: listBinding(\AppSettings.mountArgs))
            }
            settingsCard(title: "Transfer behavior", detail: "Applied to transfers started from the browser", symbol: "slider.horizontal.3") {
                labeledField("Upload arguments", text: listBinding(\AppSettings.defaultUploadArgs))
                labeledField("Download arguments", text: listBinding(\AppSettings.defaultDownloadArgs))
                Toggle("Notify when transfers finish", isOn: $draft.notifyFinishedTransfers)
            }
        }
    }

    private var advancedSettings: some View {
        Group {
            settingsCard(title: "Global arguments", detail: "Passed to every rclone invocation", symbol: "terminal") {
                labeledField("Arguments", text: listBinding(\AppSettings.advancedArgs))
                Text("Keep this empty unless you know the exact rclone flag semantics.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            settingsCard(title: "Proxy", detail: "Optional network environment for rclone", symbol: "network") {
                Toggle("Use proxy settings", isOn: $draft.useProxy)
                if draft.useProxy {
                    labeledField("HTTP", text: $draft.httpProxy)
                    labeledField("HTTPS", text: $draft.httpsProxy)
                    labeledField("No proxy", text: $draft.noProxy)
                }
            }
            settingsCard(title: "Listing exports", detail: "Filters for CSV and text directory exports", symbol: "doc.text") {
                Toggle("Stay on one filesystem", isOn: $draft.exportOptions.oneFileSystem)
                HStack {
                    TextField("Minimum size", text: $draft.exportOptions.minSize)
                    TextField("Minimum age", text: $draft.exportOptions.minAge)
                    TextField("Maximum age", text: $draft.exportOptions.maxAge)
                }
                .textFieldStyle(.roundedBorder)
                Stepper(
                    "Maximum depth: \(exportDepthLabel)",
                    value: $draft.exportOptions.maxDepth,
                    in: 0...1000
                )
                labeledField("Excludes (one per line)", text: multilineBinding(\ExportOptions.excludes))
                labeledField("Extra arguments", text: nestedListBinding(\ExportOptions.extraArgs))
            }
            settingsCard(title: "Updates", detail: "Keep the command-line engine current", symbol: "arrow.triangle.2.circlepath") {
                Toggle("Check rclone updates", isOn: $draft.checkRcloneUpdates)
                Toggle("Check app updates", isOn: $draft.checkAppUpdates)
                HStack {
                    Button("Check rclone Now") { Task { await model.checkRcloneUpdate() } }
                    Button("Check App Now") { Task { await model.checkAppUpdate() } }
                }
                .controlSize(.small)
            }
        }
    }

    private func settingsCard<Content: View>(title: String, detail: String, symbol: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                SymbolBadge(symbol: symbol, size: 31)
                SectionHeading(title: title, detail: detail)
            }
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .appCard()
    }

    private func settingRow<Content: View>(_ title: String, detail: String? = nil, @ViewBuilder content: () -> Content) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                if let detail { Text(detail).font(.caption).foregroundStyle(.secondary) }
            }
            Spacer()
            content()
        }
    }

    private func labeledField(_ title: String, text: Binding<String>, button: String? = nil, action: (() -> Void)? = nil) -> some View {
        HStack {
            Text(title).frame(width: 120, alignment: .leading)
            TextField(title, text: text).textFieldStyle(.roundedBorder)
            if let button, let action { Button(button, action: action).controlSize(.small) }
        }
    }

    private func labeledOptionalField(_ title: String, text: Binding<String?>, button: String? = nil, action: (() -> Void)? = nil) -> some View {
        labeledField(title, text: Binding(get: { text.wrappedValue ?? "" }, set: { text.wrappedValue = $0.isEmpty ? nil : $0 }), button: button, action: action)
    }

    private func listBinding(_ keyPath: WritableKeyPath<AppSettings, [String]>) -> Binding<String> {
        Binding(get: { draft[keyPath: keyPath].joined(separator: " ") }, set: { draft[keyPath: keyPath] = $0.split(whereSeparator: \.isWhitespace).map(String.init) })
    }

    private func nestedListBinding(_ keyPath: WritableKeyPath<ExportOptions, [String]>) -> Binding<String> {
        Binding(get: { draft.exportOptions[keyPath: keyPath].joined(separator: " ") }, set: { draft.exportOptions[keyPath: keyPath] = $0.split(whereSeparator: \.isWhitespace).map(String.init) })
    }

    private func multilineBinding(_ keyPath: WritableKeyPath<ExportOptions, [String]>) -> Binding<String> {
        Binding(get: { draft.exportOptions[keyPath: keyPath].joined(separator: "\n") }, set: { draft.exportOptions[keyPath: keyPath] = $0.split(whereSeparator: \.isNewline).map(String.init) })
    }

    private var exportDepthLabel: String {
        draft.exportOptions.maxDepth == 0 ? "Unlimited" : String(draft.exportOptions.maxDepth)
    }

    private func chooseRclone() {
        let panel = NSOpenPanel(); panel.canChooseFiles = true; panel.canChooseDirectories = false
        if panel.runModal() == .OK, let url = panel.url { draft.rclonePath = url.path }
    }

    private func chooseConfig() {
        let panel = NSOpenPanel(); panel.canChooseFiles = true; panel.canChooseDirectories = false
        if panel.runModal() == .OK, let url = panel.url { draft.configPath = url.path }
    }

    private func chooseDirectory(for keyPath: WritableKeyPath<AppSettings, String?>) {
        let panel = NSOpenPanel(); panel.canChooseFiles = false; panel.canChooseDirectories = true
        if panel.runModal() == .OK, let url = panel.url { draft[keyPath: keyPath] = url.path }
    }
}
