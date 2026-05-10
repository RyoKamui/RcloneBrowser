import SwiftUI

enum AppDesign {
    static let accent = Color(red: 0.30, green: 0.42, blue: 0.96)
    static let appSurfaceNSColor = NSColor(name: nil) { appearance in
        let match = appearance.bestMatch(from: [.darkAqua, .aqua])
        return match == .darkAqua
            ? NSColor(srgbRed: 35 / 255, green: 32 / 255, blue: 39 / 255, alpha: 1)
            : .white
    }
    static var appSurface: Color { Color(nsColor: appSurfaceNSColor) }
    static let sidebarSelection = Color.accentColor.opacity(0.13)
    static let hairline = Color.primary.opacity(0.09)
    static let subdued = Color.secondary.opacity(0.72)
}

struct CardStyle: ViewModifier {
    var padding: CGFloat = 16

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(AppDesign.hairline)
            }
    }
}

extension View {
    func appCard(padding: CGFloat = 16) -> some View { modifier(CardStyle(padding: padding)) }
}

struct SymbolBadge: View {
    let symbol: String
    var tint: Color = .accentColor
    var size: CGFloat = 28

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: size * 0.45, weight: .semibold))
            .foregroundStyle(tint)
            .frame(width: size, height: size)
            .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous))
    }
}

struct ToolbarIconButton: View {
    let symbol: String
    let help: String
    var disabled = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .medium))
                .frame(width: 26, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(disabled ? Color.secondary.opacity(0.4) : Color.primary.opacity(0.82))
        .background(Color.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
        .disabled(disabled)
        .help(help)
    }
}

struct SectionHeading: View {
    let title: String
    var detail: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.headline)
            if let detail { Text(detail).font(.subheadline).foregroundStyle(.secondary) }
        }
    }
}
