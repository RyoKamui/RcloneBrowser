import AppKit

@MainActor
final class AppLifecycleDelegate: NSObject, NSApplicationDelegate {
    weak var model: AppModel?

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        !(model?.settings.closeToTray ?? false)
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model else { return .terminateNow }
        if model.permitsTermination || model.runningCount == 0 { return .terminateNow }
        StatusBarController.shared.showWindow()
        model.showQuitConfirmation = true
        return .terminateCancel
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        if !flag { StatusBarController.shared.showWindow() }
        return true
    }
}

@MainActor
final class StatusBarController: NSObject {
    static let shared = StatusBarController()

    private weak var model: AppModel?
    private var statusItem: NSStatusItem?

    func configure(model: AppModel) {
        self.model = model
        apply(model.settings)
    }

    func apply(_ settings: AppSettings) {
        let visible = settings.alwaysShowTray || settings.closeToTray
        if visible, statusItem == nil {
            let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
            item.button?.image = NSImage(
                systemSymbolName: "externaldrive.connected.to.line.below",
                accessibilityDescription: "Rclone Browser"
            )
            let menu = NSMenu()
            menu.addItem(withTitle: "Show Rclone Browser", action: #selector(show), keyEquivalent: "")
            menu.addItem(withTitle: "Show Activity", action: #selector(showActivity), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: "Quit Rclone Browser", action: #selector(quit), keyEquivalent: "q")
            for item in menu.items { item.target = self }
            item.menu = menu
            statusItem = item
        } else if !visible, let statusItem {
            NSStatusBar.system.removeStatusItem(statusItem)
            self.statusItem = nil
        }
    }

    func showWindow() {
        NSApplication.shared.activate(ignoringOtherApps: true)
        if let window = NSApplication.shared.windows.first(where: { $0.canBecomeKey }) {
            window.makeKeyAndOrderFront(nil)
        }
    }

    @objc private func show() {
        showWindow()
    }

    @objc private func showActivity() {
        model?.selectedSection = .activity
        showWindow()
    }

    @objc private func quit() {
        showWindow()
        model?.requestQuit()
    }
}
