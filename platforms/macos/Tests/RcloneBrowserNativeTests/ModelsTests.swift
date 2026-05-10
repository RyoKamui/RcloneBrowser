import XCTest
@testable import RcloneBrowserNative

final class ModelsTests: XCTestCase {
    func testSettingsRoundTripPreservesEveryField() throws {
        var expected = AppSettings.fallback
        expected.rclonePath = "/opt/homebrew/bin/rclone"
        expected.configPath = "/tmp/rclone.conf"
        expected.defaultDownloadArgs = ["--checksum"]
        expected.defaultUploadArgs = ["--immutable"]
        expected.theme = .dark
        expected.dualPane = false
        expected.showTransferShelf = false
        expected.compactRows = false
        expected.exportOptions.excludes = ["*.tmp", ".DS_Store"]

        let data = try JSONEncoder().encode(expected)
        let decoded = try JSONDecoder().decode(AppSettings.self, from: data)

        XCTAssertEqual(decoded, expected)
    }

    func testTaskDefaultsMatchTheTransferEditor() {
        let task = SavedTask.blank(source: "source:path", destination: "/tmp/target")

        XCTAssertEqual(task.operation, .copy)
        XCTAssertEqual(task.source, "source:path")
        XCTAssertEqual(task.destination, "/tmp/target")
        XCTAssertEqual(task.transfers, 4)
        XCTAssertEqual(task.checkers, 8)
        XCTAssertEqual(task.connectTimeoutSeconds, 60)
        XCTAssertEqual(task.idleTimeoutSeconds, 300)
        XCTAssertEqual(task.retries, 3)
        XCTAssertEqual(task.lowLevelRetries, 10)
    }

    func testFileSymbolsCoverKnownMediaAndFolders() {
        XCTAssertEqual(entry("folder", isDirectory: true).symbol, "folder.fill")
        XCTAssertEqual(entry("photo.heic").symbol, "photo")
        XCTAssertEqual(entry("movie.mkv").symbol, "film")
        XCTAssertEqual(entry("audio.flac").symbol, "waveform")
        XCTAssertEqual(entry("archive.zip").symbol, "archivebox")
        XCTAssertEqual(entry("manual.pdf").symbol, "doc.richtext")
        XCTAssertEqual(entry("notes.txt").symbol, "doc")
    }

    func testRcloneUpdateContractMatchesSharedCoreJSON() throws {
        let data = Data(#"{"currentVersion":"1.74.4","stable":{"version":"1.75.0","released":"2026-07-31","downloadUrl":"https://downloads.rclone.org/v1.75.0"},"beta":{"version":"1.76.0-beta.10147.f0b210a88","released":"2026-08-14","downloadUrl":"https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88"},"stableUpdateAvailable":true}"#.utf8)

        let update = try JSONDecoder().decode(RcloneUpdateInfo.self, from: data)

        XCTAssertEqual(update.currentVersion, "1.74.4")
        XCTAssertEqual(update.stable?.version, "1.75.0")
        XCTAssertEqual(update.stable?.downloadURL, "https://downloads.rclone.org/v1.75.0")
        XCTAssertEqual(update.beta?.version, "1.76.0-beta.10147.f0b210a88")
        XCTAssertEqual(update.beta?.downloadURL, "https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88")
        XCTAssertTrue(update.stableUpdateAvailable)
    }

    private func entry(_ name: String, isDirectory: Bool = false) -> BrowserEntry {
        BrowserEntry(
            name: name,
            path: name,
            isDir: isDirectory,
            size: nil,
            modTime: nil,
            mimeType: nil
        )
    }
}

@MainActor
final class PaneStateTests: XCTestCase {
    func testNavigationHistoryDropsTheForwardBranch() {
        let pane = PaneState(id: .primary, remote: "remote", path: "")
        pane.navigate(remote: "remote", path: "one")
        pane.navigate(remote: "remote", path: "one/two")

        XCTAssertEqual(pane.goBack()?.path, "one")
        XCTAssertTrue(pane.canGoForward)

        pane.navigate(remote: "remote", path: "replacement")

        XCTAssertEqual(pane.path, "replacement")
        XCTAssertFalse(pane.canGoForward)
    }

    func testTabsKeepIndependentLocationsAndSharedState() {
        let pane = PaneState(id: .primary, remote: "drive", path: "first")
        let originalID = pane.activeTabID
        pane.toggleSharedWithMe()
        pane.newTab(remote: "archive", path: "second")
        let secondID = pane.activeTabID

        XCTAssertEqual(pane.remote, "archive")
        XCTAssertFalse(pane.sharedWithMe)

        pane.selectTab(originalID)
        XCTAssertEqual(pane.path, "first")
        XCTAssertTrue(pane.sharedWithMe)

        pane.closeTab(originalID)
        XCTAssertEqual(pane.activeTabID, secondID)
        XCTAssertEqual(pane.tabs.count, 1)
    }

    func testFilteringAndSortingKeepFoldersFirst() {
        let pane = PaneState(id: .primary, remote: "remote", path: "")
        pane.entries = [
            BrowserEntry(name: "zeta.txt", path: "zeta.txt", isDir: false, size: 20, modTime: "2026-01-02", mimeType: nil),
            BrowserEntry(name: "Folder", path: "Folder", isDir: true, size: nil, modTime: "2026-01-01", mimeType: nil),
            BrowserEntry(name: "alpha.txt", path: "alpha.txt", isDir: false, size: 10, modTime: "2026-01-03", mimeType: nil),
        ]

        XCTAssertEqual(pane.visibleEntries.map(\.name), ["Folder", "alpha.txt", "zeta.txt"])

        pane.sort = .size
        pane.sortAscending = false
        XCTAssertEqual(pane.visibleEntries.map(\.name), ["Folder", "zeta.txt", "alpha.txt"])

        pane.search = "alpha"
        XCTAssertEqual(pane.visibleEntries.map(\.name), ["alpha.txt"])
    }

    func testEntryCacheIsScopedByLocationAndSharedMode() {
        let pane = PaneState(id: .primary, remote: "drive", path: "first")
        let first = BrowserEntry(name: "first.txt", path: "first.txt", isDir: false, size: 1, modTime: nil, mimeType: nil)
        pane.cache([first])

        pane.navigate(remote: "drive", path: "second")
        XCTAssertNil(pane.cachedEntries())

        pane.navigate(remote: "drive", path: "first")
        XCTAssertEqual(pane.cachedEntries(), [first])

        pane.toggleSharedWithMe()
        XCTAssertNil(pane.cachedEntries())

        pane.clearCache()
        pane.toggleSharedWithMe()
        XCTAssertNil(pane.cachedEntries())
    }
}
