import SwiftUI
import UIKit

struct AppearanceLinkageControls: View {
    @EnvironmentObject private var coordinator: AppearanceCoordinator
    @State private var selected = 0
    @ScaledMetric(relativeTo: .caption) private var minimumNodeWidth = 56
    @ScaledMetric(relativeTo: .caption) private var connectionWidth = 44
    @ScaledMetric(relativeTo: .caption) private var chainHeight = 104
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.dynamicTypeSize) private var typeSize
    private var value: AppearanceLinkage { coordinator.value }
    var body: some View {
        Section(L("appearance.link_title")) {
            GeometryReader { geometry in
                let width = max(geometry.size.width, minimumNodeWidth * 3 + connectionWidth * 2 + 16)
                let nodeWidth = (width - connectionWidth * 2 - 16) / 3
                ScrollView(.horizontal) {
                    HStack(spacing: 4) {
                        node(0, value.source == .system ? "circle.lefthalf.filled" : "sun.max", "source", L(value.source.labelKey), width: nodeWidth)
                        connection(app: true)
                        node(1, "rectangle.on.rectangle", "app", L(coordinator.appDark ? "appearance.mode_dark" : "appearance.mode_light"), width: nodeWidth)
                        connection(app: false)
                        node(2, "book", "reading", L(coordinator.readingDark ? "appearance.mode_dark" : "appearance.mode_light"), width: nodeWidth)
                    }.frame(width: width)
                }.scrollIndicators(typeSize.isAccessibilitySize ? .visible : .hidden)
            }.frame(height: chainHeight)
                .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
                .listRowSeparator(.hidden)
            VStack(alignment: .leading, spacing: 12) {
                Text(L(selected == 0 ? "appearance.link_source" : selected == 1 ? "appearance.link_app" : "appearance.link_reading"))
                    .font(.subheadline.weight(.semibold))
                if selected == 0 {
                    sourcePicker("appearance.link_basis", selection: $coordinator.value.source)
                    if value.appLinked { sourceNote(value.source) }
                    else { Text(L("appearance.link_app_independent")).font(.footnote).foregroundStyle(.secondary) }
                } else if selected == 1 {
                    if value.appLinked { LabeledContent(L("appearance.link_follow_source"), value: L(value.source.labelKey)).font(.subheadline) }
                    else {
                        adaptivePicker {
                            Picker(L("appearance.link_app"), selection: $coordinator.value.appManual) {
                                Text(L("appearance.mode_light")).tag(false); Text(L("appearance.mode_dark")).tag(true)
                            }
                        }
                    }
                } else if value.readingLinked {
                    Text(L("appearance.link_follow_app")).font(.subheadline).foregroundStyle(.secondary)
                } else {
                    adaptivePicker {
                        Picker(L("appearance.link_reading"), selection: $coordinator.value.readingMode) {
                            ForEach(ReadingDisplayMode.allCases, id: \.self) { Text(L($0.labelKey)).tag($0) }
                        }
                    }
                    if value.readingMode == .automatic {
                        if !typeSize.isAccessibilitySize {
                            Text(L("appearance.link_reading_basis")).font(.footnote).foregroundStyle(.secondary)
                        }
                        sourcePicker("appearance.link_reading_basis", selection: $coordinator.value.readingSource)
                        sourceNote(value.readingSource)
                    }
                }
            }.padding(.vertical, 4)
        }
    }
    private func sourcePicker(_ key: String, selection: Binding<AutomaticAppearanceSource>) -> some View {
        adaptivePicker {
            Picker(L(key), selection: selection) { ForEach(AutomaticAppearanceSource.allCases, id: \.self) { Text(L($0.labelKey)).tag($0) } }
        }
    }
    @ViewBuilder private func adaptivePicker<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        if typeSize.isAccessibilitySize { content().pickerStyle(.menu) }
        else { content().pickerStyle(.segmented) }
    }
    @ViewBuilder private func sourceNote(_ source: AutomaticAppearanceSource) -> some View {
        if source == .ambient {
            Text(L(!coordinator.sensorAvailable ? "appearance.link_unavailable" : coordinator.ambientDark == nil ? "appearance.link_waiting" : "appearance.link_ambient_hint"))
                .font(.footnote).foregroundStyle(.secondary)
        }
    }
    private func node(_ index: Int, _ symbol: String, _ title: String, _ state: String, width: CGFloat) -> some View {
        let mode = index == 0 ? "" : index == 1 ? L(value.appLinked ? "appearance.link_follow_source" : "appearance.link_independent") : value.readingLinked ? L("appearance.link_follow_app") : L(value.readingMode == .automatic ? "appearance.mode_automatic" : "appearance.link_independent")
        return Button { selected = index } label: {
            VStack(spacing: 7) {
                Image(systemName: symbol).font(.title3).foregroundStyle(selected == index ? Color.accentColor : Color.primary)
                Text(L("appearance.link_\(title)")).font(.caption.weight(.medium)).foregroundStyle(.primary)
                Text(state).font(.caption2).foregroundStyle(.secondary)
            }.multilineTextAlignment(.center).frame(width: width, height: chainHeight)
                .background(selected == index ? Color.accentColor.opacity(0.10) : Color(uiColor: .secondarySystemFill).opacity(0.45), in: RoundedRectangle(cornerRadius: 14))
                .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(selected == index ? Color.accentColor.opacity(0.3) : Color.primary.opacity(0.10), lineWidth: 1) }
                .contentShape(Rectangle())
        }.buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel([L("appearance.link_\(title)"), mode, state].filter { !$0.isEmpty }.joined(separator: "、"))
            .accessibilityAddTraits(.isButton)
            .accessibilityAddTraits(selected == index ? [.isSelected] : [])
            .accessibilityIdentifier("appearance-node-\(title)")
    }
    private func connection(app: Bool) -> some View {
        let linked = app ? value.appLinked : value.readingLinked
        let statusKey = app ? (linked ? "appearance.link_app_linked" : "appearance.link_app_independent") : (linked ? "appearance.link_reading_linked" : "appearance.link_reading_independent")
        return Button {
            withAnimation(.easeInOut(duration: reduceMotion ? 0 : 0.18)) {
                if app { coordinator.toggleAppLink(); selected = 1 }
                else { coordinator.toggleReadingLink(); selected = 2 }
            }
        } label: {
            VStack(spacing: 5) {
                Image(systemName: linked ? "link" : (UIImage(systemName: "link.badge.plus") != nil ? "link.badge.plus" : "arrow.triangle.2.circlepath")).font(.body)
                Text(L(linked ? "appearance.link_unlink" : "appearance.link_relink")).font(.caption2)
                    .multilineTextAlignment(.center)
            }.foregroundStyle(Color.accentColor)
                .frame(width: connectionWidth, height: chainHeight).contentShape(Rectangle())
        }.buttonStyle(.plain)
            .accessibilityLabel(L(app ? (linked ? "appearance.link_unlink_app" : "appearance.link_relink_app") : (linked ? "appearance.link_unlink_reading" : "appearance.link_relink_reading")))
            .accessibilityValue(L(statusKey))
            .accessibilityIdentifier(app ? "appearance-edge-app" : "appearance-edge-reading")
    }
}

struct AppearanceLinkageSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var coordinator: AppearanceCoordinator
    var body: some View {
        NavigationStack { Form { AppearanceLinkageControls() }
            .navigationTitle(L("appearance.link_title")).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("manage.done")) { dismiss() } } }
        }.readerPanelSurface(appearance: coordinator).environment(\.colorScheme, coordinator.appDark ? .dark : .light)
    }
}
