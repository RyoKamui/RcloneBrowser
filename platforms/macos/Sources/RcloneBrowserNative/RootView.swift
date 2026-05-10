import AppKit
import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel
    @State private var sidebarWidth: CGFloat = 232
    @State private var sidebarWidthAtDragStart: CGFloat?

    var body: some View {
        HStack(spacing: 0) {
            SidebarView()
                .frame(width: sidebarWidth)

            sidebarDivider
            detail
        }
        .background(AppDesign.appSurface)
        .overlay {
            if model.isBootstrapping {
                ZStack {
                    Rectangle().fill(.ultraThinMaterial)
                    VStack(spacing: 14) {
                        ProgressView().controlSize(.small)
                        Text("Opening your workspace…").font(.callout).foregroundStyle(.secondary)
                    }
                    .padding(24)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                }
                .ignoresSafeArea()
            }
        }
        .alert("Rclone Browser", isPresented: errorPresented) {
            Button("OK") { model.globalError = nil }
        } message: {
            Text(model.globalError ?? "")
        }
        .alert("Rclone Browser", isPresented: noticePresented) {
            Button("OK") { model.notice = nil }
        } message: {
            Text(model.notice ?? "")
        }
        .confirmationDialog("Transfers are still running", isPresented: $model.showQuitConfirmation) {
            Button("Quit and Cancel Work", role: .destructive) { Task { await model.quitAndCancelWork() } }
            Button("Keep Running", role: .cancel) {}
        } message: {
            Text("Quitting now will stop active transfers, mounts, and streams.")
        }
        .sheet(isPresented: $model.showAddLocation) { LocationWizardView() }
        .sheet(isPresented: $model.showMount) { MountSheet() }
        .sheet(isPresented: $model.showReconfigure) {
            if let remote = model.reconfiguringRemote { ReconfigureLocationView(remote: remote) }
        }
        .sheet(isPresented: $model.showTextPreview) { TextPreviewSheet() }
        .sheet(isPresented: $model.showRcloneUpdate) { RcloneUpdateSheet() }
        .sheet(isPresented: $model.showTaskEditor) {
            TaskEditorView(task: model.editingTask ?? .blank(
                source: model.endpoint(remote: model.currentPane.remote, path: model.currentPane.path),
                destination: model.endpoint(remote: model.otherPane.remote, path: model.otherPane.path)
            ))
        }
    }

    private var detail: some View {
        Group {
            switch model.selectedSection {
            case .workspace: WorkspaceView()
            case .activity: ActivityView()
            case .tasks: TasksView()
            case .settings: SettingsView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AppDesign.appSurface)
    }

    private var sidebarDivider: some View {
        Rectangle()
            .fill(AppDesign.hairline)
            .frame(width: 1)
            .background {
                Rectangle()
                    .fill(AppDesign.hairline)
                    .ignoresSafeArea(edges: .top)
            }
            .overlay {
                Color.clear
                    .frame(width: 9)
                    .contentShape(Rectangle())
                    .onHover { hovering in
                        if hovering { NSCursor.resizeLeftRight.push() }
                        else { NSCursor.pop() }
                    }
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { value in
                                let initial = sidebarWidthAtDragStart ?? sidebarWidth
                                sidebarWidthAtDragStart = initial
                                sidebarWidth = min(max(initial + value.translation.width, 206), 310)
                            }
                            .onEnded { _ in sidebarWidthAtDragStart = nil }
                    )
            }
    }

    private var errorPresented: Binding<Bool> {
        Binding(get: { model.globalError != nil }, set: { if !$0 { model.globalError = nil } })
    }

    private var noticePresented: Binding<Bool> {
        Binding(get: { model.notice != nil }, set: { if !$0 { model.notice = nil } })
    }
}
