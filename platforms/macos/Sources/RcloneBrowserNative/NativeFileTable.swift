import AppKit
import SwiftUI

enum FileTableAction: Int {
    case open
    case copyToOtherPane
    case moveToOtherPane
    case transferOptions
    case download
    case copyPath
    case rename
    case moveWithinLocation
    case publicLink
    case calculateSize
    case showTree
    case exportListing
    case mount
    case stream
    case delete
}

struct NativeFileTable: NSViewRepresentable {
    var entries: [BrowserEntry]
    @Binding var selectedIDs: Set<String>
    var compactRows: Bool
    var showFolderIcons: Bool
    var showFileIcons: Bool
    var alternatingRows: Bool
    var iconSize: IconSize
    var sort: FileSort
    var sortAscending: Bool
    var isLocal: Bool
    var isReadOnly: Bool
    var onFocus: () -> Void
    var onOpen: (BrowserEntry) -> Void
    var onDelete: () -> Void
    var onSort: (FileSort, Bool) -> Void
    var onAction: (BrowserEntry, FileTableAction) -> Void
    var onDrop: ([URL]) -> Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let table = BrowserTableView()
        table.delegate = context.coordinator
        table.dataSource = context.coordinator
        table.target = context.coordinator
        table.doubleAction = #selector(Coordinator.openDoubleClickedRow)
        table.allowsMultipleSelection = true
        table.allowsEmptySelection = true
        table.allowsColumnSelection = false
        table.allowsColumnReordering = false
        table.allowsColumnResizing = true
        table.allowsTypeSelect = true
        table.usesAlternatingRowBackgroundColors = alternatingRows && !entries.isEmpty
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.selectionHighlightStyle = .regular
        table.style = .fullWidth
        table.backgroundColor = AppDesign.appSurfaceNSColor
        table.columnAutoresizingStyle = .firstColumnOnlyAutoresizingStyle

        let name = NSTableColumn(identifier: .fileName)
        name.title = "Name"
        name.minWidth = 180
        name.width = 420
        name.resizingMask = [.autoresizingMask, .userResizingMask]
        name.sortDescriptorPrototype = NSSortDescriptor(key: "name", ascending: true)

        let size = NSTableColumn(identifier: .fileSize)
        size.title = "Size"
        size.minWidth = 72
        size.maxWidth = 130
        size.width = 86
        size.resizingMask = .userResizingMask
        size.sortDescriptorPrototype = NSSortDescriptor(key: "size", ascending: true)

        let modified = NSTableColumn(identifier: .fileModified)
        modified.title = "Modified"
        modified.minWidth = 118
        modified.maxWidth = 220
        modified.width = 145
        modified.resizingMask = .userResizingMask
        modified.sortDescriptorPrototype = NSSortDescriptor(key: "modified", ascending: true)

        table.addTableColumn(name)
        table.addTableColumn(size)
        table.addTableColumn(modified)
        table.headerView = NSTableHeaderView()
        table.focusHandler = onFocus
        table.openHandler = context.coordinator.openSelectedRow
        table.deleteHandler = onDelete
        table.contextMenuHandler = context.coordinator.contextMenu(for:)
        table.dropHandler = onDrop
        table.registerForDraggedTypes([.fileURL])

        let scroll = BrowserScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.drawsBackground = true
        scroll.backgroundColor = AppDesign.appSurfaceNSColor
        scroll.columnLayoutHandler = { [weak coordinator = context.coordinator] width in
            coordinator?.resizeColumns(to: width)
        }
        context.coordinator.table = table
        context.coordinator.entries = entries
        context.coordinator.applySortIndicator()
        return scroll
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let table = context.coordinator.table else { return }
        let previous = context.coordinator.parent
        context.coordinator.parent = self
        table.focusHandler = onFocus
        table.deleteHandler = onDelete
        table.dropHandler = onDrop
        table.usesAlternatingRowBackgroundColors = alternatingRows && !entries.isEmpty
        table.rowHeight = compactRows ? 29 : 36
        table.backgroundColor = AppDesign.appSurfaceNSColor
        scrollView.backgroundColor = AppDesign.appSurfaceNSColor

        let appearanceChanged = previous.compactRows != compactRows
            || previous.showFolderIcons != showFolderIcons
            || previous.showFileIcons != showFileIcons
            || previous.iconSize != iconSize
            || previous.alternatingRows != alternatingRows
        if context.coordinator.entries != entries {
            context.coordinator.entries = entries
            table.reloadData()
        } else if appearanceChanged {
            table.reloadData()
        }

        context.coordinator.applySortIndicator()
        context.coordinator.reconcileSelection(selectedIDs)
        context.coordinator.resizeColumns(to: scrollView.contentSize.width)
    }

    @MainActor
    final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
        var parent: NativeFileTable
        var entries: [BrowserEntry] = []
        fileprivate weak var table: BrowserTableView?
        private var applyingSelection = false
        private var nativeSelectionInFlight: Set<String>?
        private var contextEntry: BrowserEntry?

        init(parent: NativeFileTable) {
            self.parent = parent
        }

        func numberOfRows(in tableView: NSTableView) -> Int {
            entries.count
        }

        func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
            parent.compactRows ? 29 : 36
        }

        func tableView(
            _ tableView: NSTableView,
            viewFor tableColumn: NSTableColumn?,
            row: Int
        ) -> NSView? {
            guard entries.indices.contains(row), let tableColumn else { return nil }
            let entry = entries[row]
            switch tableColumn.identifier {
            case .fileName:
                let identifier = NSUserInterfaceItemIdentifier("FileNameCell")
                let cell = (tableView.makeView(withIdentifier: identifier, owner: nil) as? FileNameCell)
                    ?? FileNameCell(identifier: identifier)
                let showIcon = entry.isDir ? parent.showFolderIcons : parent.showFileIcons
                cell.configure(entry: entry, showIcon: showIcon, iconSize: parent.iconSize)
                return cell
            case .fileSize:
                return textCell(
                    in: tableView,
                    identifier: "FileSizeCell",
                    text: entry.size.map {
                        ByteCountFormatter.string(fromByteCount: Int64($0), countStyle: .file)
                    } ?? "—"
                )
            case .fileModified:
                return textCell(
                    in: tableView,
                    identifier: "FileModifiedCell",
                    text: Self.dateText(entry.modTime)
                )
            default:
                return nil
            }
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !applyingSelection, let table else { return }
            let selection = Set(table.selectedRowIndexes.compactMap { index in
                entries.indices.contains(index) ? entries[index].id : nil
            })
            if parent.selectedIDs != selection {
                nativeSelectionInFlight = selection
                parent.selectedIDs = selection
            }
        }

        func reconcileSelection(_ selectedIDs: Set<String>) {
            if let nativeSelectionInFlight {
                guard nativeSelectionInFlight == selectedIDs else { return }
                self.nativeSelectionInFlight = nil
            }
            applySelection(selectedIDs)
        }

        func tableView(_ tableView: NSTableView, sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]) {
            guard let descriptor = tableView.sortDescriptors.first else { return }
            let option: FileSort
            switch descriptor.key {
            case "size": option = .size
            case "modified": option = .modified
            default: option = .name
            }
            parent.onSort(option, descriptor.ascending)
        }

        func applySelection(_ selectedIDs: Set<String>) {
            guard let table else { return }
            let indexes = IndexSet(entries.indices.filter { selectedIDs.contains(entries[$0].id) })
            guard table.selectedRowIndexes != indexes else { return }
            applyingSelection = true
            if indexes.isEmpty { table.deselectAll(nil) }
            else { table.selectRowIndexes(indexes, byExtendingSelection: false) }
            applyingSelection = false
        }

        func applySortIndicator() {
            guard let table else { return }
            let key: String
            switch parent.sort {
            case .name: key = "name"
            case .size: key = "size"
            case .modified: key = "modified"
            }
            let next = NSSortDescriptor(key: key, ascending: parent.sortAscending)
            if table.sortDescriptors.first?.key != key
                || table.sortDescriptors.first?.ascending != parent.sortAscending {
                table.sortDescriptors = [next]
            }
        }

        func resizeColumns(to width: CGFloat) {
            guard let table, width >= 360, table.tableColumns.count == 3 else { return }
            let sizeWidth: CGFloat = 82
            let modifiedWidth: CGFloat = 140
            table.tableColumns[1].width = sizeWidth
            table.tableColumns[2].width = modifiedWidth
            table.tableColumns[0].width = max(180, width - sizeWidth - modifiedWidth)
            if abs(table.frame.width - width) > 0.5 {
                table.setFrameSize(NSSize(width: width, height: table.frame.height))
            }
        }

        @objc func openDoubleClickedRow() {
            openSelectedRow()
        }

        func openSelectedRow() {
            guard let table, entries.indices.contains(table.selectedRow) else { return }
            parent.onOpen(entries[table.selectedRow])
        }

        func contextMenu(for row: Int) -> NSMenu? {
            guard entries.indices.contains(row) else { return nil }
            let entry = entries[row]
            contextEntry = entry
            let menu = NSMenu()
            add(entry.isDir ? "Open" : (parent.isLocal ? "Open" : "Stream"), .open, to: menu)
            menu.addItem(.separator())
            add("Copy to Other Pane", .copyToOtherPane, to: menu)
            add("Move to Other Pane", .moveToOtherPane, to: menu)
            add("Transfer Options…", .transferOptions, to: menu)
            add("Download…", .download, to: menu)
            add("Copy rclone Path", .copyPath, to: menu)
            if !parent.isReadOnly {
                menu.addItem(.separator())
                add("Rename…", .rename, to: menu)
                add("Move Within Location…", .moveWithinLocation, to: menu)
            }
            if !parent.isLocal {
                add("Copy Public Link", .publicLink, to: menu)
            }
            if entry.isDir {
                menu.addItem(.separator())
                add("Calculate Size", .calculateSize, to: menu)
                add("Show Directory Tree", .showTree, to: menu)
                add("Export Listing…", .exportListing, to: menu)
                if !parent.isLocal { add("Mount Folder…", .mount, to: menu) }
            } else if !parent.isLocal {
                add("Stream", .stream, to: menu)
            }
            if !parent.isReadOnly {
                menu.addItem(.separator())
                let item = add("Delete", .delete, to: menu)
                item.attributedTitle = NSAttributedString(
                    string: "Delete",
                    attributes: [.foregroundColor: NSColor.systemRed]
                )
            }
            return menu
        }

        @discardableResult
        private func add(_ title: String, _ action: FileTableAction, to menu: NSMenu) -> NSMenuItem {
            let item = NSMenuItem(title: title, action: #selector(performContextAction(_:)), keyEquivalent: "")
            item.target = self
            item.tag = action.rawValue
            menu.addItem(item)
            return item
        }

        @objc private func performContextAction(_ sender: NSMenuItem) {
            guard let entry = contextEntry, let action = FileTableAction(rawValue: sender.tag) else { return }
            parent.onAction(entry, action)
        }

        private func textCell(in tableView: NSTableView, identifier value: String, text: String) -> NSTableCellView {
            let identifier = NSUserInterfaceItemIdentifier(value)
            let cell = (tableView.makeView(withIdentifier: identifier, owner: nil) as? NSTableCellView)
                ?? Self.makeTextCell(identifier: identifier)
            cell.textField?.stringValue = text
            cell.textField?.font = .systemFont(ofSize: parent.compactRows ? 11.5 : 12.5)
            return cell
        }

        private static func makeTextCell(identifier: NSUserInterfaceItemIdentifier) -> NSTableCellView {
            let cell = NSTableCellView()
            cell.identifier = identifier
            let label = NSTextField(labelWithString: "")
            label.alignment = .right
            label.textColor = .secondaryLabelColor
            label.lineBreakMode = .byTruncatingTail
            label.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(label)
            cell.textField = label
            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 6),
                label.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
                label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
            return cell
        }

        private static func dateText(_ value: String?) -> String {
            guard let value else { return "—" }
            let date = (try? Date(value, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)))
                ?? (try? Date(value, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: false)))
            guard let date else { return "—" }
            return date.formatted(date: .abbreviated, time: .shortened)
        }
    }
}

private final class BrowserTableView: NSTableView {
    var focusHandler: (() -> Void)?
    var openHandler: (() -> Void)?
    var deleteHandler: (() -> Void)?
    var contextMenuHandler: ((Int) -> NSMenu?)?
    var dropHandler: (([URL]) -> Bool)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        super.mouseDown(with: event)
        // Let AppKit finish its complete selection-tracking cycle before a
        // SwiftUI pane-focus update can cause this representable to refresh.
        focusHandler?()
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 36, 76:
            openHandler?()
        case 51, 117:
            deleteHandler?()
        default:
            super.keyDown(with: event)
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let row = row(at: convert(event.locationInWindow, from: nil))
        guard row >= 0 else { return nil }
        focusHandler?()
        if !selectedRowIndexes.contains(row) {
            selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        }
        return contextMenuHandler?(row)
    }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggedFileURLs(from: sender).isEmpty ? [] : .copy
    }

    override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggedFileURLs(from: sender).isEmpty ? [] : .copy
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        let urls = draggedFileURLs(from: sender)
        return urls.isEmpty ? false : dropHandler?(urls) ?? false
    }

    private func draggedFileURLs(from sender: any NSDraggingInfo) -> [URL] {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [.urlReadingFileURLsOnly: true]
        let values = sender.draggingPasteboard.readObjects(
            forClasses: [NSURL.self],
            options: options
        ) as? [NSURL]
        return values?.map { $0 as URL } ?? []
    }
}

private final class BrowserScrollView: NSScrollView {
    var columnLayoutHandler: ((CGFloat) -> Void)?

    override func layout() {
        super.layout()
        columnLayoutHandler?(contentSize.width)
    }
}

private final class FileNameCell: NSTableCellView {
    private let symbol = NSImageView()
    private let label = NSTextField(labelWithString: "")
    private var symbolWidth: NSLayoutConstraint!

    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
        symbol.imageScaling = .scaleProportionallyDown
        symbol.contentTintColor = .secondaryLabelColor
        symbol.translatesAutoresizingMaskIntoConstraints = false
        label.lineBreakMode = .byTruncatingMiddle
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(symbol)
        addSubview(label)
        imageView = symbol
        textField = label
        symbolWidth = symbol.widthAnchor.constraint(equalToConstant: 19)
        NSLayoutConstraint.activate([
            symbol.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            symbol.centerYAnchor.constraint(equalTo: centerYAnchor),
            symbolWidth,
            symbol.heightAnchor.constraint(equalToConstant: 20),
            label.leadingAnchor.constraint(equalTo: symbol.trailingAnchor, constant: 8),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { nil }

    func configure(entry: BrowserEntry, showIcon: Bool, iconSize: IconSize) {
        label.stringValue = entry.name
        label.font = .systemFont(ofSize: 12.5)
        symbol.isHidden = !showIcon
        symbolWidth.constant = showIcon ? 19 : 0
        guard showIcon else { symbol.image = nil; return }
        let pointSize: CGFloat
        switch iconSize {
        case .small: pointSize = 11
        case .medium: pointSize = 14
        case .large: pointSize = 17
        }
        let configuration = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .regular)
        symbol.image = NSImage(systemSymbolName: entry.symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(configuration)
        symbol.contentTintColor = entry.isDir ? .controlAccentColor : .secondaryLabelColor
    }
}

private extension NSUserInterfaceItemIdentifier {
    static let fileName = NSUserInterfaceItemIdentifier("FileNameColumn")
    static let fileSize = NSUserInterfaceItemIdentifier("FileSizeColumn")
    static let fileModified = NSUserInterfaceItemIdentifier("FileModifiedColumn")
}
