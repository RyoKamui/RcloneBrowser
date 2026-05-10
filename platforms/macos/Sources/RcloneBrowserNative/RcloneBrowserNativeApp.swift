import SwiftUI

@main
struct RcloneBrowserNativeApp: App {
    @NSApplicationDelegateAdaptor(AppLifecycleDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("Rclone Browser") {
            RootView()
                .environmentObject(model)
                .preferredColorScheme(colorScheme)
                .frame(minWidth: 980, minHeight: 640)
                .task {
                    appDelegate.model = model
                    StatusBarController.shared.configure(model: model)
                    model.start()
                }
        }
        .defaultSize(width: 1280, height: 800)
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Tab") { model.addTab(model.currentPane) }
                    .keyboardShortcut("t", modifiers: .command)
                Button("New Folder…") {
                    NotificationCenter.default.post(name: .newFolderRequested, object: nil)
                }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
            }
            CommandMenu("Transfer") {
                Button("Copy to Other Pane") { Task { await model.transferSelection(.copy) } }
                    .keyboardShortcut("c", modifiers: [.command, .option])
                Button("Move to Other Pane") { Task { await model.transferSelection(.move) } }
                    .keyboardShortcut("m", modifiers: [.command, .option])
                Divider()
                Button("Show Activity") { model.selectedSection = .activity }
                    .keyboardShortcut("0", modifiers: [.command, .shift])
            }
            CommandGroup(replacing: .appTermination) {
                Button("Quit Rclone Browser") { model.requestQuit() }
                    .keyboardShortcut("q", modifiers: .command)
            }
        }
    }

    private var colorScheme: ColorScheme? {
        // Deterministic appearance overrides are useful for screenshot and UI
        // regression tests; normal launches continue to honor Settings.
        if ProcessInfo.processInfo.arguments.contains("--force-dark") { return .dark }
        if ProcessInfo.processInfo.arguments.contains("--force-light") { return .light }
        switch model.settings.theme {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}

extension Notification.Name {
    static let newFolderRequested = Notification.Name("RcloneBrowser.newFolderRequested")
}
