import Foundation

enum AppTheme: String, Codable, CaseIterable, Identifiable, Hashable {
    case system, light, dark
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

enum IconSize: String, Codable, CaseIterable, Identifiable, Hashable {
    case small, medium, large
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

struct ExportOptions: Codable, Equatable {
    var oneFileSystem: Bool
    var minSize: String
    var minAge: String
    var maxAge: String
    var maxDepth: UInt32
    var excludes: [String]
    var extraArgs: [String]
}

struct AppSettings: Codable, Equatable {
    var rclonePath: String
    var configPath: String?
    var defaultDownloadDir: String?
    var defaultUploadDir: String?
    var defaultDownloadArgs: [String]
    var defaultUploadArgs: [String]
    var showHidden: Bool
    var showFolderIcons: Bool
    var showFileIcons: Bool
    var alternatingRows: Bool
    var iconSize: IconSize
    var confirmDelete: Bool
    var theme: AppTheme
    var advancedArgs: [String]
    var streamCommand: String
    var mountArgs: [String]
    var closeToTray: Bool
    var alwaysShowTray: Bool
    var notifyFinishedTransfers: Bool
    var checkAppUpdates: Bool
    var checkRcloneUpdates: Bool
    var useProxy: Bool
    var httpProxy: String
    var httpsProxy: String
    var noProxy: String
    var exportOptions: ExportOptions
    var dualPane: Bool
    var showTransferShelf: Bool
    var compactRows: Bool

    static let fallback = AppSettings(
        rclonePath: "rclone", configPath: nil, defaultDownloadDir: nil, defaultUploadDir: nil,
        defaultDownloadArgs: [], defaultUploadArgs: [], showHidden: true,
        showFolderIcons: true, showFileIcons: true, alternatingRows: true,
        iconSize: .medium, confirmDelete: true, theme: .system, advancedArgs: [],
        streamCommand: "mpv -", mountArgs: ["--vfs-cache-mode", "writes"],
        closeToTray: false, alwaysShowTray: false,
        notifyFinishedTransfers: true, checkAppUpdates: true, checkRcloneUpdates: true,
        useProxy: false, httpProxy: "", httpsProxy: "", noProxy: "",
        exportOptions: ExportOptions(oneFileSystem: false, minSize: "", minAge: "", maxAge: "", maxDepth: 0, excludes: [], extraArgs: []),
        dualPane: true, showTransferShelf: true, compactRows: true
    )
}

struct RcloneStatus: Codable {
    var available: Bool
    var version: String?
    var error: String?
}

struct RcloneRemote: Codable, Identifiable, Hashable {
    var name: String
    var type: String
    var description: String
    var isLocal: Bool
    var displayName: String
    var id: String { name }

    var symbol: String {
        switch type.lowercased() {
        case "local": return "macbook"
        case "drive", "onedrive", "dropbox", "box": return "cloud"
        case "s3", "b2", "azureblob", "swift": return "externaldrive"
        case "sftp", "ftp", "webdav": return "server.rack"
        case "crypt": return "lock"
        default: return "network"
        }
    }
}

struct BrowserEntry: Codable, Identifiable, Hashable {
    var name: String
    var path: String
    var isDir: Bool
    var size: UInt64?
    var modTime: String?
    var mimeType: String?
    var id: String { path }

    var symbol: String {
        if isDir { return "folder.fill" }
        let ext = URL(fileURLWithPath: name).pathExtension.lowercased()
        if ["jpg", "jpeg", "png", "gif", "heic", "webp", "svg"].contains(ext) { return "photo" }
        if ["mp4", "mov", "mkv", "avi", "webm"].contains(ext) { return "film" }
        if ["mp3", "m4a", "flac", "wav", "aac"].contains(ext) { return "waveform" }
        if ["zip", "7z", "rar", "tar", "gz"].contains(ext) { return "archivebox" }
        if ["pdf"].contains(ext) { return "doc.richtext" }
        return "doc"
    }
}

struct ConfigProvider: Codable, Identifiable, Hashable {
    var name: String
    var description: String
    var prefix: String
    var hide: Bool
    var id: String { name }
}

struct ConfigExample: Codable, Hashable {
    var value: String
    var help: String
}

struct ConfigOption: Codable, Hashable {
    var name: String
    var help: String
    var defaultStr: String
    var valueStr: String
    var required: Bool
    var isPassword: Bool
    var exclusive: Bool
    var sensitive: Bool
    var optionType: String
    var examples: [ConfigExample]
}

struct ConfigQuestion: Codable, Hashable {
    var state: String
    var option: ConfigOption?
    var error: String
    var result: String
}

enum TransferDirection: String, Codable, CaseIterable, Identifiable, Hashable {
    case upload, download, copy
    var id: String { rawValue }
}

enum TransferOperation: String, Codable, CaseIterable, Identifiable, Hashable {
    case copy, move, sync
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
    var symbol: String { self == .move ? "arrow.right" : self == .sync ? "arrow.triangle.2.circlepath" : "doc.on.doc" }
}

enum WorkStatus: String, Codable, Hashable {
    case queued, running, completed, failed, cancelled
    var isRunning: Bool { self == .queued || self == .running }
}

struct TransferRequest: Codable {
    var direction: TransferDirection
    var operation: TransferOperation
    var source: String
    var destination: String
    var isDirectory: Bool
    var extraArgs: [String]
    var label: String?
}

struct TransferSnapshot: Codable, Identifiable, Hashable {
    var id: String
    var direction: TransferDirection
    var operation: TransferOperation
    var label: String?
    var source: String
    var destination: String
    var isDirectory: Bool
    var extraArgs: [String]
    var status: WorkStatus
    var bytes: UInt64
    var totalBytes: UInt64?
    var speed: Double?
    var etaSeconds: Double?
    var checks: UInt64
    var totalChecks: UInt64?
    var filesTransferred: UInt64
    var totalFiles: UInt64?
    var errors: UInt64
    var elapsedSeconds: Double?
    var startedAt: UInt64
    var finishedAt: UInt64?
    var error: String?
    var logTail: [String]

    var fraction: Double? {
        guard let totalBytes, totalBytes > 0 else { return nil }
        return min(Double(bytes) / Double(totalBytes), 1)
    }
}

enum ActivityKind: String, Codable, Hashable { case mount, stream }

struct ActivitySnapshot: Codable, Identifiable, Hashable {
    var id: String
    var kind: ActivityKind
    var source: String
    var destination: String
    var status: WorkStatus
    var startedAt: UInt64
    var finishedAt: UInt64?
    var error: String?
    var logTail: [String]
}

enum CompareMode: String, Codable, CaseIterable, Identifiable, Hashable {
    case sizeAndModTime, checksum, ignoreSize, sizeOnly, checksumIgnoreSize
    var id: String { rawValue }
}

enum SyncDeleteMode: String, Codable, CaseIterable, Identifiable, Hashable {
    case during, after, before
    var id: String { rawValue }
}

struct SavedTask: Codable, Identifiable, Hashable {
    var id: String
    var description: String
    var direction: TransferDirection
    var operation: TransferOperation
    var source: String
    var destination: String
    var isDirectory: Bool
    var syncDeleteMode: SyncDeleteMode?
    var update: Bool
    var ignoreExisting: Bool
    var compareMode: CompareMode
    var oneFileSystem: Bool
    var noUpdateModtime: Bool
    var transfers: UInt16
    var checkers: UInt16
    var bandwidth: String
    var minSize: String
    var minAge: String
    var maxAge: String
    var maxDepth: UInt32
    var connectTimeoutSeconds: UInt32
    var idleTimeoutSeconds: UInt32
    var retries: UInt16
    var lowLevelRetries: UInt16
    var deleteExcluded: Bool
    var excludes: [String]
    var extraArgs: [String]
    var sharedWithMe: Bool

    static func blank(source: String = "", destination: String = "") -> SavedTask {
        SavedTask(id: UUID().uuidString, description: "", direction: .copy, operation: .copy,
                  source: source, destination: destination, isDirectory: true,
                  syncDeleteMode: nil, update: false, ignoreExisting: false,
                  compareMode: .sizeAndModTime, oneFileSystem: false, noUpdateModtime: false,
                  transfers: 4, checkers: 8, bandwidth: "", minSize: "", minAge: "", maxAge: "",
                  maxDepth: 0, connectTimeoutSeconds: 60, idleTimeoutSeconds: 300,
                  retries: 3, lowLevelRetries: 10, deleteExcluded: false,
                  excludes: [], extraArgs: [], sharedWithMe: false)
    }
}

struct DirectorySummary: Codable {
    var count: UInt64
    var bytes: UInt64
}

struct RcloneRelease: Codable, Equatable {
    var version: String
    var released: String?
    var downloadURL: String?

    enum CodingKeys: String, CodingKey {
        case version, released
        case downloadURL = "downloadUrl"
    }
}

struct RcloneUpdateInfo: Codable, Equatable {
    var currentVersion: String
    var stable: RcloneRelease?
    var beta: RcloneRelease?
    var stableUpdateAvailable: Bool
}

struct Bootstrap: Codable {
    var appVersion: String
    var settings: AppSettings
    var rclone: RcloneStatus
    var remotes: [RcloneRemote]
    var transfers: [TransferSnapshot]
    var activities: [ActivitySnapshot]
    var tasks: [SavedTask]
    var dataDirectory: String
}

struct BrowserLocation: Hashable {
    var remote: String
    var path: String
    var sharedWithMe = false
}

struct BrowserTab: Identifiable, Hashable {
    var id = UUID()
    var remote: String
    var path: String
    var sharedWithMe = false

    var title: String {
        let name = path.split(separator: "/").last.map(String.init)
        return name?.isEmpty == false ? name! : (remote == "__local__" ? "Mac" : remote)
    }
}

enum FileSort: String, CaseIterable, Identifiable {
    case name, size, modified
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

enum SidebarSection: Hashable {
    case workspace
    case activity
    case tasks
    case settings
}

enum PaneID: String { case primary, secondary }

struct EmptyPayload: Codable {}
struct NamePayload: Codable { var name: String }
struct IDPayload: Codable { var id: String }
struct PasswordPayload: Codable { var password: String }
struct BrowserPayload: Codable { var remote: String; var path: String; var sharedWithMe = false }
struct PathPayload: Codable { var remote: String; var path: String; var sharedWithMe = false }
struct RenamePayload: Codable { var remote: String; var path: String; var newName: String; var sharedWithMe = false }
struct MovePayload: Codable { var remote: String; var source: String; var destination: String; var sharedWithMe = false }
struct DeletePayload: Codable { var remote: String; var path: String; var isDir: Bool; var sharedWithMe = false }
struct StartConfigPayload: Codable { var name: String; var provider: String }
struct ContinueConfigPayload: Codable { var name: String; var provider: String; var state: String; var result: String }
struct ContinueUpdatePayload: Codable { var name: String; var state: String; var result: String }
struct ExportPayload: Codable { var remote: String; var path: String; var destination: String; var format: String; var sharedWithMe = false }
struct MountPayload: Codable { var source: String; var destination: String; var extraArgs: [String] = [] }
struct StreamPayload: Codable { var source: String; var command: String = "" }
struct RunTaskPayload: Codable { var id: String; var dryRun: Bool }
struct StartTaskPayload: Codable { var task: SavedTask; var dryRun: Bool }
