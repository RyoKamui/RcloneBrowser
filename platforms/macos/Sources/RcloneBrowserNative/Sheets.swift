import AppKit
import SwiftUI

struct LocationWizardView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var search = ""
    @State private var selectedProvider: ConfigProvider?
    @State private var question: ConfigQuestion?
    @State private var answer = ""
    @State private var isWorking = false
    @State private var localError: String?
    @State private var configStarted = false
    @State private var completed = false

    var filteredProviders: [ConfigProvider] {
        guard !search.isEmpty else { return model.providers }
        return model.providers.filter {
            $0.description.localizedStandardContains(search) || $0.name.localizedStandardContains(search)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            Group {
                if let question { questionStep(question) }
                else { providerStep }
            }
            Divider()
            footer
        }
        .frame(width: 680, height: 650)
        .interactiveDismissDisabled(isWorking)
        .alert("Couldn’t Add Location", isPresented: Binding(get: { localError != nil }, set: { if !$0 { localError = nil } })) {
            Button("OK") { localError = nil }
        } message: { Text(localError ?? "") }
        .onDisappear {
            if configStarted && !completed {
                let pendingName = name
                Task.detached { try? RustBridge.callVoid("cancelConfig", payload: NamePayload(name: pendingName)) }
            }
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            SymbolBadge(symbol: "network.badge.shield.half.filled", size: 38)
            SectionHeading(
                title: question == nil ? "Add a Location" : (selectedProvider?.description ?? "Configure Location"),
                detail: question == nil ? "Every protocol installed with rclone is available." : "Answer rclone’s setup questions without leaving the app."
            )
            Spacer()
        }
        .padding(.horizontal, 22)
        .frame(height: 76)
    }

    private var providerStep: some View {
        VStack(alignment: .leading, spacing: 17) {
            VStack(alignment: .leading, spacing: 7) {
                Text("LOCATION NAME").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                TextField("For example, Work Drive", text: $name)
                    .textFieldStyle(.roundedBorder)
                    .font(.body)
            }
            VStack(alignment: .leading, spacing: 7) {
                Text("STORAGE PROTOCOL").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Find Google Drive, S3, SFTP, WebDAV…", text: $search).textFieldStyle(.plain)
                    if !search.isEmpty {
                        Button { search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary) }.buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 10).frame(height: 32)
                .background(Color.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 8))
            }
            if model.providers.isEmpty {
                Spacer()
                ProgressView("Loading rclone protocols…").frame(maxWidth: .infinity)
                Spacer()
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                        ForEach(filteredProviders) { provider in
                            ProviderCard(provider: provider, selected: selectedProvider?.id == provider.id) {
                                selectedProvider = provider
                            }
                        }
                    }
                    .padding(1)
                }
            }
        }
        .padding(22)
    }

    private func questionStep(_ question: ConfigQuestion) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let option = question.option {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(option.name.isEmpty ? "Configuration" : readable(option.name))
                            .font(.title3.weight(.semibold))
                        if !option.help.isEmpty {
                            Text(option.help).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                        }
                        if option.required {
                            Text("Required").font(.caption2.weight(.semibold)).foregroundStyle(.orange)
                        }
                    }

                    answerControl(option)

                    if !option.examples.isEmpty && !option.exclusive {
                        VStack(alignment: .leading, spacing: 7) {
                            Text("SUGGESTED VALUES").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                            ForEach(option.examples, id: \.self) { example in
                                Button {
                                    answer = example.value
                                } label: {
                                    HStack {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(example.value).font(.system(.callout, design: .monospaced))
                                            if !example.help.isEmpty { Text(example.help).font(.caption).foregroundStyle(.secondary) }
                                        }
                                        Spacer()
                                        if answer == example.value {
                                            Image(systemName: "checkmark.circle.fill")
                                                .foregroundStyle(Color.accentColor)
                                        }
                                    }
                                    .padding(10)
                                    .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 9))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                } else {
                    ContentUnavailableView("Finishing Setup", systemImage: "gearshape.2", description: Text("rclone is completing this location."))
                }
            }
            .padding(26)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder
    private func answerControl(_ option: ConfigOption) -> some View {
        if option.exclusive && !option.examples.isEmpty {
            Picker("Value", selection: $answer) {
                if !option.required { Text("Use default").tag("") }
                ForEach(option.examples, id: \.self) { example in
                    Text(example.help.isEmpty ? example.value : "\(example.value) — \(example.help)").tag(example.value)
                }
            }
            .labelsHidden()
            .frame(maxWidth: .infinity, alignment: .leading)
        } else if option.isPassword || option.sensitive {
            SecureField(option.defaultStr.isEmpty ? "Value" : "Default: \(option.defaultStr)", text: $answer)
                .textFieldStyle(.roundedBorder)
        } else if option.optionType.lowercased().contains("bool") {
            Picker("Value", selection: $answer) {
                Text("Use default").tag("")
                Text("Yes").tag("true")
                Text("No").tag("false")
            }
            .pickerStyle(.segmented).frame(width: 300)
        } else {
            TextField(option.defaultStr.isEmpty ? "Value" : "Default: \(option.defaultStr)", text: $answer)
                .textFieldStyle(.roundedBorder)
                .onSubmit(continueSetup)
        }
    }

    private var footer: some View {
        HStack {
            Text(selectedProvider.map { "\(name.isEmpty ? "New location" : name) · \($0.name)" } ?? "Choose a protocol to continue")
                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
            Spacer()
            Button("Cancel") { cancel() }.keyboardShortcut(.cancelAction)
            if question == nil {
                Button("Continue", action: startSetup)
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || selectedProvider == nil || isWorking)
            } else {
                Button("Next", action: continueSetup)
                    .keyboardShortcut(.defaultAction)
                    .disabled(isWorking || ((question?.option?.required ?? false) && answer.isEmpty && (question?.option?.defaultStr.isEmpty ?? true)))
            }
            if isWorking { ProgressView().controlSize(.small).padding(.leading, 5) }
        }
        .padding(.horizontal, 22)
        .frame(height: 58)
    }

    private func startSetup() {
        guard let provider = selectedProvider else { return }
        let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        isWorking = true
        Task {
            do {
                let next: ConfigQuestion = try await Task.detached {
                    try RustBridge.call("startConfig", payload: StartConfigPayload(name: cleanName, provider: provider.name))
                }.value
                configStarted = true
                await accept(next)
            } catch { localError = error.localizedDescription }
            isWorking = false
        }
    }

    private func continueSetup() {
        guard let provider = selectedProvider, let current = question else { return }
        let result = answer.isEmpty ? (current.option?.valueStr.isEmpty == false ? current.option!.valueStr : current.option?.defaultStr ?? "") : answer
        let locationName = name
        isWorking = true
        Task {
            do {
                let next: ConfigQuestion = try await Task.detached {
                    try RustBridge.call("continueConfig", payload: ContinueConfigPayload(name: locationName, provider: provider.name, state: current.state, result: result))
                }.value
                await accept(next)
            } catch { localError = error.localizedDescription }
            isWorking = false
        }
    }

    private func accept(_ next: ConfigQuestion) async {
        if next.state.isEmpty {
            completed = true
            await model.completeLocation()
            dismiss()
        } else {
            question = next
            answer = next.option?.valueStr.isEmpty == false ? next.option!.valueStr : ""
        }
    }

    private func cancel() {
        let pendingName = name
        let shouldDelete = configStarted && !completed
        completed = true
        Task {
            if shouldDelete {
                try? await Task.detached { try RustBridge.callVoid("cancelConfig", payload: NamePayload(name: pendingName)) }.value
            }
            dismiss()
        }
    }

    private func readable(_ value: String) -> String {
        value.replacingOccurrences(of: "_", with: " ").split(separator: " ").map { $0.capitalized }.joined(separator: " ")
    }
}

private struct ProviderCard: View {
    let provider: ConfigProvider
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 11) {
                SymbolBadge(symbol: symbol, tint: selected ? .accentColor : .secondary, size: 34)
                VStack(alignment: .leading, spacing: 3) {
                    Text(provider.description.isEmpty ? provider.name : provider.description)
                        .font(.system(size: 12.5, weight: .medium)).lineLimit(2)
                    Text(provider.name).font(.caption2.monospaced()).foregroundStyle(.secondary)
                }
                Spacer(minLength: 2)
                if selected { Image(systemName: "checkmark.circle.fill").foregroundStyle(Color.accentColor) }
            }
            .padding(11)
            .frame(maxWidth: .infinity, minHeight: 59, alignment: .leading)
            .background(selected ? Color.accentColor.opacity(0.1) : Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: 11).strokeBorder(selected ? Color.accentColor.opacity(0.55) : AppDesign.hairline) }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var symbol: String {
        switch provider.name.lowercased() {
        case "drive", "onedrive", "dropbox", "box": return "cloud"
        case "s3", "b2", "azureblob", "swift": return "externaldrive"
        case "sftp", "ftp", "webdav": return "server.rack"
        case "crypt": return "lock"
        case "local": return "macbook"
        default: return "network"
        }
    }
}

struct ReconfigureLocationView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let remote: RcloneRemote
    @State private var question: ConfigQuestion?
    @State private var answer = ""
    @State private var isWorking = true
    @State private var localError: String?
    @State private var started = false
    @State private var completed = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                SymbolBadge(symbol: "arrow.triangle.2.circlepath", size: 38)
                SectionHeading(title: "Reconfigure \(remote.displayName)", detail: "Review provider options and refresh OAuth when the provider requests it.")
                Spacer()
            }
            .padding(.horizontal, 22).frame(height: 76)
            Divider()
            Group {
                if let option = question?.option {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 20) {
                            VStack(alignment: .leading, spacing: 5) {
                                Text(readable(option.name)).font(.title3.weight(.semibold))
                                if !option.help.isEmpty { Text(option.help).font(.callout).foregroundStyle(.secondary).textSelection(.enabled) }
                                if option.required { Text("Required").font(.caption2.weight(.semibold)).foregroundStyle(.orange) }
                            }
                            answerControl(option)
                        }
                        .padding(26).frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else {
                    VStack(spacing: 13) {
                        ProgressView().controlSize(.small)
                        Text("Preparing rclone options…").font(.callout).foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            Divider()
            HStack {
                Text("Changes are written only after rclone finishes the sequence.").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel", action: cancel).keyboardShortcut(.cancelAction)
                Button("Next", action: submit)
                    .keyboardShortcut(.defaultAction)
                    .disabled(question == nil || isWorking || ((question?.option?.required ?? false) && answer.isEmpty && (question?.option?.defaultStr.isEmpty ?? true)))
                if isWorking { ProgressView().controlSize(.small).padding(.leading, 5) }
            }
            .padding(.horizontal, 22).frame(height: 58)
        }
        .frame(width: 650, height: 560)
        .interactiveDismissDisabled(isWorking)
        .task { await begin() }
        .alert("Couldn’t Reconfigure Location", isPresented: Binding(get: { localError != nil }, set: { if !$0 { localError = nil } })) {
            Button("OK") { localError = nil }
        } message: { Text(localError ?? "") }
        .onDisappear {
            if started && !completed {
                let name = remote.name
                Task.detached { try? RustBridge.callVoid("cancelUpdate", payload: NamePayload(name: name)) }
            }
        }
    }

    @ViewBuilder
    private func answerControl(_ option: ConfigOption) -> some View {
        if option.exclusive && !option.examples.isEmpty {
            Picker("Value", selection: $answer) {
                if !option.required { Text("Use default").tag("") }
                ForEach(option.examples, id: \.self) { example in
                    Text(example.help.isEmpty ? example.value : "\(example.value) — \(example.help)").tag(example.value)
                }
            }
            .labelsHidden().frame(maxWidth: .infinity, alignment: .leading)
        } else if option.isPassword || option.sensitive {
            SecureField(option.defaultStr.isEmpty ? "Value" : "Default is set", text: $answer).textFieldStyle(.roundedBorder)
        } else if option.optionType.lowercased().contains("bool") {
            Picker("Value", selection: $answer) {
                Text("Use default").tag("")
                Text("Yes").tag("true")
                Text("No").tag("false")
            }
            .pickerStyle(.segmented).frame(width: 300)
        } else {
            TextField(option.defaultStr.isEmpty ? "Value" : "Default: \(option.defaultStr)", text: $answer)
                .textFieldStyle(.roundedBorder).onSubmit(submit)
        }
    }

    private func begin() async {
        do {
            let name = remote.name
            let next: ConfigQuestion = try await Task.detached {
                try RustBridge.call("startUpdate", payload: NamePayload(name: name))
            }.value
            started = true
            await accept(next)
        } catch { localError = error.localizedDescription }
        isWorking = false
    }

    private func submit() {
        guard let current = question else { return }
        let result = answer.isEmpty ? (current.option?.valueStr.isEmpty == false ? current.option!.valueStr : current.option?.defaultStr ?? "") : answer
        let name = remote.name
        isWorking = true
        Task {
            do {
                let next: ConfigQuestion = try await Task.detached {
                    try RustBridge.call("continueUpdate", payload: ContinueUpdatePayload(name: name, state: current.state, result: result))
                }.value
                await accept(next)
            } catch { localError = error.localizedDescription }
            isWorking = false
        }
    }

    private func accept(_ next: ConfigQuestion) async {
        if next.state.isEmpty {
            completed = true
            await model.completeReconfiguration()
            dismiss()
        } else {
            question = next
            answer = next.option?.valueStr.isEmpty == false ? next.option!.valueStr : ""
        }
    }

    private func cancel() {
        let name = remote.name
        completed = true
        Task {
            if started { try? await Task.detached { try RustBridge.callVoid("cancelUpdate", payload: NamePayload(name: name)) }.value }
            dismiss()
        }
    }

    private func readable(_ value: String) -> String {
        value.replacingOccurrences(of: "_", with: " ").split(separator: " ").map { $0.capitalized }.joined(separator: " ")
    }
}

struct MountSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var source = ""
    @State private var destination = ""
    @State private var extraArguments = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 11) {
                SymbolBadge(symbol: "externaldrive.badge.plus", size: 38)
                SectionHeading(title: "Mount Location", detail: "Expose an rclone path as a folder on this Mac.")
            }
            VStack(alignment: .leading, spacing: 7) {
                Text("SOURCE").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                TextField("remote:path", text: $source).textFieldStyle(.roundedBorder)
            }
            VStack(alignment: .leading, spacing: 7) {
                Text("MOUNT FOLDER").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                HStack {
                    TextField("Folder on this Mac", text: $destination).textFieldStyle(.roundedBorder)
                    Button("Choose…", action: chooseDestination)
                }
            }
            DisclosureGroup("Additional options") {
                TextField("Extra rclone mount arguments", text: $extraArguments).textFieldStyle(.roundedBorder).padding(.top, 8)
            }
            Spacer()
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Mount") {
                    Task { await model.startMount(source: source, destination: destination, extraArgs: extraArguments.split(whereSeparator: \.isWhitespace).map(String.init)) }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(source.isEmpty || destination.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 530, height: 370)
        .onAppear {
            source = model.mountSource.isEmpty
                ? model.endpoint(remote: model.currentPane.remote, path: model.currentPane.path)
                : model.mountSource
            extraArguments = model.mountPresetArguments.joined(separator: " ")
        }
    }

    private func chooseDestination() {
        let panel = NSOpenPanel(); panel.canChooseFiles = false; panel.canChooseDirectories = true
        if panel.runModal() == .OK, let url = panel.url { destination = url.path }
    }
}

struct TaskEditorView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var task: SavedTask
    @State private var showAdvanced = false

    init(task: SavedTask) { _task = State(initialValue: task) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 11) {
                SymbolBadge(symbol: task.operation.symbol, size: 38)
                SectionHeading(title: task.description.isEmpty ? "New Saved Task" : "Edit Saved Task", detail: "A repeatable rclone operation with explicit safety options.")
                Spacer()
            }
            .padding(.horizontal, 22).frame(height: 76)
            Divider()
            Form {
                Section("Essentials") {
                    TextField("Task name", text: $task.description)
                    Picker("Direction", selection: $task.direction) {
                        Text("Between locations").tag(TransferDirection.copy)
                        Text("Upload").tag(TransferDirection.upload)
                        Text("Download").tag(TransferDirection.download)
                    }
                    Picker("Operation", selection: $task.operation) { ForEach(TransferOperation.allCases) { Text($0.title).tag($0) } }
                    TextField("Source", text: $task.source)
                    TextField("Destination", text: $task.destination)
                    Toggle("Source is a folder", isOn: $task.isDirectory)
                }
                Section("Transfer policy") {
                    Toggle("Update newer files only", isOn: $task.update)
                    Toggle("Ignore existing files", isOn: $task.ignoreExisting)
                    Picker("Compare using", selection: $task.compareMode) {
                        Text("Size and modified time").tag(CompareMode.sizeAndModTime)
                        Text("Checksum").tag(CompareMode.checksum)
                        Text("Size only").tag(CompareMode.sizeOnly)
                        Text("Ignore size").tag(CompareMode.ignoreSize)
                        Text("Checksum and ignore size").tag(CompareMode.checksumIgnoreSize)
                    }
                    if task.operation == .sync {
                        Picker("Delete timing", selection: Binding(get: { task.syncDeleteMode ?? .during }, set: { task.syncDeleteMode = $0 })) {
                            Text("During").tag(SyncDeleteMode.during)
                            Text("After").tag(SyncDeleteMode.after)
                            Text("Before").tag(SyncDeleteMode.before)
                        }
                    }
                }
                Section("Performance") {
                    Stepper("Parallel transfers: \(task.transfers)", value: $task.transfers, in: 1...64)
                    Stepper("Checkers: \(task.checkers)", value: $task.checkers, in: 1...64)
                    TextField("Bandwidth limit", text: $task.bandwidth, prompt: Text("For example, 10M"))
                }
                Section {
                    DisclosureGroup("Advanced filters and retries", isExpanded: $showAdvanced) {
                        Toggle("Stay on one filesystem", isOn: $task.oneFileSystem)
                        Toggle("Do not update modification times", isOn: $task.noUpdateModtime)
                        Toggle("Delete excluded files", isOn: $task.deleteExcluded)
                        TextField("Minimum size", text: $task.minSize)
                        TextField("Minimum age", text: $task.minAge)
                        TextField("Maximum age", text: $task.maxAge)
                        Stepper("Retries: \(task.retries)", value: $task.retries, in: 1...100)
                        Stepper("Low-level retries: \(task.lowLevelRetries)", value: $task.lowLevelRetries, in: 1...100)
                        Stepper("Maximum depth: \(task.maxDepth == 0 ? "Unlimited" : String(task.maxDepth))", value: $task.maxDepth, in: 0...100)
                        Stepper("Connect timeout: \(task.connectTimeoutSeconds)s", value: $task.connectTimeoutSeconds, in: 1...3600)
                        Stepper("Idle timeout: \(task.idleTimeoutSeconds)s", value: $task.idleTimeoutSeconds, in: 1...7200)
                        TextField("Excludes (space separated)", text: Binding(get: { task.excludes.joined(separator: " ") }, set: { task.excludes = $0.split(whereSeparator: \.isWhitespace).map(String.init) }))
                        TextField("Extra arguments", text: Binding(get: { task.extraArgs.joined(separator: " ") }, set: { task.extraArgs = $0.split(whereSeparator: \.isWhitespace).map(String.init) }))
                        Toggle("Google Drive shared with me", isOn: $task.sharedWithMe)
                    }
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Button("Copy Command") { Task { await model.copyCommand(for: task) } }
                    .disabled(task.source.isEmpty || task.destination.isEmpty)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Dry Run") { Task { await model.runTaskDraft(task, dryRun: true) } }
                    .disabled(task.source.isEmpty || task.destination.isEmpty)
                Button("Run") { Task { await model.runTaskDraft(task) } }
                    .disabled(task.source.isEmpty || task.destination.isEmpty)
                Button("Save Task") { Task { await model.saveTask(task) } }
                    .keyboardShortcut(.defaultAction)
                    .disabled(task.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || task.source.isEmpty || task.destination.isEmpty)
            }
            .padding(.horizontal, 22).frame(height: 58)
        }
        .frame(width: 650, height: 720)
    }
}

struct TextPreviewSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                SectionHeading(title: model.textPreviewTitle)
                Spacer()
                Button("Copy") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(model.textPreview, forType: .string)
                }
                .controlSize(.small)
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20).frame(height: 62)
            Divider()
            ScrollView([.horizontal, .vertical]) {
                Text(model.textPreview)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
            }
        }
        .frame(width: 720, height: 520)
    }
}

struct RcloneUpdateSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    private var info: RcloneUpdateInfo? { model.rcloneUpdateInfo }
    private var stableIsNewer: Bool {
        info?.stableUpdateAvailable ?? false
    }

    var body: some View {
        VStack(spacing: 0) {
            VStack(spacing: 10) {
                Image(systemName: stableIsNewer ? "arrow.down.circle.fill" : "checkmark.circle.fill")
                    .font(.system(size: 34, weight: .regular))
                    .foregroundStyle(stableIsNewer ? Color.accentColor : Color.green)
                Text(stableIsNewer ? "rclone Update Available" : "rclone Is Up to Date")
                    .font(.title3.weight(.semibold))
                if let current = info?.currentVersion {
                    Text("Installed version \(current)")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.top, 26)
            .padding(.bottom, 22)

            if let stable = info?.stable {
                Divider()
                releaseRow(title: "Stable", release: stable, recommended: stableIsNewer)
            }
            if let beta = info?.beta {
                Divider()
                releaseRow(title: "Beta", release: beta, recommended: false)
            }

            Divider()
            HStack {
                Text("Beta builds may be less stable.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20)
            .frame(height: 58)
        }
        .frame(width: 440)
    }

    private func releaseRow(title: String, release: RcloneRelease, recommended: Bool) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 7) {
                    Text(title).fontWeight(.medium)
                    if recommended {
                        Text("RECOMMENDED")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(Color.accentColor)
                    }
                }
                Text(release.released.map { "Released \($0)" } ?? "Release date unavailable")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Text(release.version)
                .font(.system(.callout, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .help(release.version)
                .frame(maxWidth: 180, alignment: .trailing)
            if let value = release.downloadURL, let url = URL(string: value) {
                Button("Download") { NSWorkspace.shared.open(url) }
                    .controlSize(.small)
            }
        }
        .padding(.horizontal, 20)
        .frame(minHeight: 68)
    }
}
