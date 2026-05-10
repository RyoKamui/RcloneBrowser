import AppKit
import SwiftUI

struct SidebarView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            brand
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    sidebarGroup("Locations", trailing: AnyView(addLocationButton)) {
                        VStack(spacing: 2) {
                            ForEach(model.remotes) { remote in
                                LocationRow(remote: remote)
                            }
                        }
                    }
                    sidebarGroup("Library") {
                        VStack(spacing: 2) {
                            SidebarButton(title: "Activity", symbol: "arrow.up.arrow.down", badge: model.runningCount, selected: model.selectedSection == .activity) {
                                model.selectedSection = .activity
                            }
                            SidebarButton(title: "Saved Tasks", symbol: "clock.arrow.circlepath", badge: model.tasks.count, selected: model.selectedSection == .tasks) {
                                model.selectedSection = .tasks
                            }
                            SidebarButton(title: "Settings", symbol: "gearshape", selected: model.selectedSection == .settings) {
                                model.selectedSection = .settings
                            }
                        }
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 14)
            }
            Divider().opacity(0.6)
            status
        }
        .background {
            Rectangle()
                .fill(AppDesign.appSurface)
                .ignoresSafeArea(edges: .top)
        }
        .clipShape(Rectangle())
    }

    private var brand: some View {
        HStack(spacing: 10) {
            Image(nsImage: NSApplication.shared.applicationIconImage)
                .resizable()
                .interpolation(.high)
                .frame(width: 36, height: 36)
            VStack(alignment: .leading, spacing: 1) {
                Text("Rclone Browser").font(.system(size: 13, weight: .semibold))
                Text("Native workspace").font(.caption2).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.top, 15)
        .padding(.bottom, 10)
    }

    private var addLocationButton: some View {
        Button {
            model.showAddLocation = true
            Task { await model.loadProviders() }
        } label: {
            Image(systemName: "plus").font(.system(size: 11, weight: .semibold))
        }
        .buttonStyle(.plain)
        .help("Add location")
    }

    private var status: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(model.rclone.available ? Color.green : Color.orange)
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 1) {
                Text(model.rclone.available ? "rclone connected" : "rclone needs attention")
                    .font(.caption).fontWeight(.medium)
                Text(model.rclone.version ?? model.rclone.error ?? "Check Settings")
                    .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
        }
        .padding(12)
        .contentShape(Rectangle())
        .onTapGesture { if !model.rclone.available { model.selectedSection = .settings } }
    }

    @ViewBuilder
    private func sidebarGroup<Content: View>(_ title: String, trailing: AnyView? = nil, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(title.uppercased())
                    .font(.system(size: 10, weight: .semibold))
                    .tracking(0.5)
                    .foregroundStyle(.secondary)
                Spacer()
                trailing
            }
            .padding(.horizontal, 8)
            content()
        }
    }
}

private struct LocationRow: View {
    @EnvironmentObject private var model: AppModel
    let remote: RcloneRemote
    @State private var confirmRemoval = false

    var isActive: Bool {
        model.selectedSection == .workspace && model.currentPane.remote == remote.name
    }

    var body: some View {
        Button {
            if model.currentPane.remote == remote.name {
                model.selectedSection = .workspace
            } else {
                model.chooseRemote(remote, pane: model.currentPane)
            }
        } label: {
            HStack(spacing: 9) {
                Image(systemName: remote.symbol)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(isActive ? Color.accentColor : Color.secondary)
                    .frame(width: 17)
                VStack(alignment: .leading, spacing: 1) {
                    Text(remote.displayName).lineLimit(1)
                    if !remote.isLocal {
                        Text(remote.type).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                Spacer()
            }
            .font(.system(size: 12.5))
            .padding(.horizontal, 9)
            .frame(height: remote.isLocal ? 31 : 38)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Open in Left Pane") { model.chooseRemote(remote, pane: model.primary) }
            Button("Open in Right Pane") { model.chooseRemote(remote, pane: model.secondary) }
            if !remote.isLocal {
                Divider()
                Button("Reconfigure…") { model.reconfigure(remote) }
                Button("Remove Location…", role: .destructive) { confirmRemoval = true }
            }
        }
        .confirmationDialog("Remove “\(remote.displayName)”?", isPresented: $confirmRemoval) {
            Button("Remove Location", role: .destructive) { Task { await model.deleteRemote(remote) } }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This removes the location from rclone. Files stored by the provider are not deleted.")
        }
    }
}

private struct SidebarButton: View {
    let title: String
    let symbol: String
    var badge: Int = 0
    var selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: symbol)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(selected ? Color.accentColor : Color.secondary)
                    .frame(width: 17)
                Text(title)
                Spacer()
                if badge > 0 {
                    Text("\(badge)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Color.primary.opacity(0.07), in: Capsule())
                }
            }
            .font(.system(size: 12.5, weight: selected ? .medium : .regular))
            .padding(.horizontal, 9)
            .frame(height: 31)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
