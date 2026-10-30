import SwiftUI

struct BookCategory: Codable, Identifiable, Sendable { let id: String; let name: String }
struct BookClassification: Codable, Sendable {
    var categories: [BookCategory] = []
    var assignments: [String: String] = [:]
    func matches(_ id: String, filter: String?) -> Bool {
        guard let filter else { return true }
        return filter.isEmpty ? assignments[id] == nil : assignments[id] == filter
    }
}

struct LibraryCategorySection: View {
    @ObservedObject var library: PreviewLibrary
    @Binding var filter: String?
    @Binding var targets: BookShareSelection?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Namespace private var glassNamespace
    @Namespace private var selectionNamespace
    @FocusState private var nameFocused: Bool
    @State private var managing = false
    @State private var editing: BookCategory?
    @State private var naming = false
    @State private var name = ""
    @State private var deleting: String?
    @State private var failure: String?
    @State private var working = false
    private var motion: Animation? { reduceMotion ? nil : .smooth(duration: 0.32, extraBounce: 0) }
    private var panelVisible: Bool { managing || targets != nil }
    private var common: String? {
        guard let ids = targets?.ids, let first = ids.first else { return nil }
        let value = library.classification.assignments[first] ?? ""
        return ids.allSatisfy { (library.classification.assignments[$0] ?? "") == value } ? value : nil
    }
    private func change(_ fields: [String: Any], done: @escaping () -> Void = {}) {
        guard !working else { return }; working = true; failure = nil
        Task {
            defer { working = false }
            do {
                let result = try await library.store.categories(fields)
                withAnimation(motion) { library.classification = result; done() }
            } catch { withAnimation(motion) { failure = error.localizedDescription } }
        }
    }
    var body: some View {
        ReaderChromeGlassContainer {
            VStack(alignment: .leading, spacing: 12) {
                filterBar
                if panelVisible {
                    surface(panel.padding(14), shape: RoundedRectangle(cornerRadius: 20, style: .continuous), id: "category-panel")
                        .disabled(working)
                        .transition(.opacity)
                }
            }
        }
        .animation(motion, value: managing)
        .animation(motion, value: targets?.ids)
        .animation(motion, value: naming)
        .animation(motion, value: deleting)
        .onChange(of: filter) { _, _ in
            withAnimation(motion) { targets = nil; managing = false; naming = false }
            nameFocused = false
        }
        .onChange(of: targets?.ids) { _, ids in
            if ids != nil {
                withAnimation(motion) { managing = false; naming = false; deleting = nil; failure = nil }
                nameFocused = false
            }
        }
        .task(id: naming) { nameFocused = naming }
    }
    private var filterBar: some View {
        HStack(spacing: 10) {
            surface(ScrollView(.horizontal) {
                HStack(spacing: 4) {
                    filterChip(L("ui.all_books"), id: nil)
                    filterChip(L("category.uncategorized"), id: "")
                    ForEach(library.classification.categories) { category in filterChip(category.name, id: category.id) }
                }.padding(4)
            }.scrollIndicators(.hidden).frame(height: 52), shape: Capsule(), id: "category-filters", interactive: true)
            surface(Button {
                withAnimation(motion) { managing.toggle(); targets = nil; failure = nil }
                if !managing { naming = false; nameFocused = false }
            } label: {
                Image(systemName: "folder.badge.gearshape")
                    .symbolVariant(managing ? .fill : .none)
                    .frame(width: 48, height: 48)
            }.buttonStyle(.plain).foregroundStyle(Color.accentColor)
                .accessibilityLabel(L("category.manage")).accessibilityAddTraits(managing ? [.isSelected] : []),
                shape: Circle(), id: "category-manager", interactive: true)
        }
    }
    private var panel: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(L(managing ? "category.manage" : "category.assign")).font(.subheadline.weight(.semibold))
                if let targets { Text(L("manage.count", targets.ids.count)).font(.caption).foregroundStyle(.secondary) }
                Spacer()
                Button(L("manage.done")) {
                    nameFocused = false
                    withAnimation(motion) {
                        if managing { managing = false; naming = false } else { targets = nil }
                    }
                }.frame(minHeight: 44)
            }
            if managing {
                Text(L("category.note")).font(.caption).foregroundStyle(.secondary)
                namingControls
                categoryList
            } else if let targets {
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        choice(L("category.uncategorized"), id: "", ids: targets.ids)
                        ForEach(library.classification.categories) { category in choice(category.name, id: category.id, ids: targets.ids) }
                    }
                }.scrollIndicators(.hidden)
            }
            if let failure { Text(failure).font(.caption).foregroundStyle(.red).accessibilityAddTraits(.updatesFrequently).transition(.opacity) }
        }
    }
    @ViewBuilder private var namingControls: some View {
        if naming {
            HStack {
                TextField(L("category.name"), text: $name).textFieldStyle(.roundedBorder).focused($nameFocused).submitLabel(.done).onSubmit(saveName)
                Button(L("error.ok"), action: saveName).disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button(L("settings.cancel")) { nameFocused = false; withAnimation(motion) { naming = false } }
            }.frame(minHeight: 44).transition(.opacity)
        } else {
            Button(L("category.create"), systemImage: "plus") { beginNaming(nil) }.frame(minHeight: 44).transition(.opacity)
        }
    }
    @ViewBuilder private var categoryList: some View {
        if !library.classification.categories.isEmpty {
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(library.classification.categories) { category in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(category.name).lineLimit(2)
                                Spacer()
                                Menu {
                                    Button(L("category.rename"), systemImage: "pencil") { beginNaming(category) }
                                    Button(L("category.delete"), systemImage: "trash", role: .destructive) {
                                        nameFocused = false
                                        withAnimation(motion) { deleting = category.id; naming = false }
                                    }
                                } label: { Image(systemName: "ellipsis").frame(width: 44, height: 44) }
                                    .accessibilityLabel(category.name + ", " + L("category.manage"))
                            }
                            if deleting == category.id {
                                VStack(alignment: .leading, spacing: 6) {
                                    Text(L("category.delete_note")).font(.caption).foregroundStyle(.secondary)
                                    HStack {
                                        Button(L("category.delete"), role: .destructive) { change(["action":"delete", "id":category.id]) { deleting = nil } }
                                        Button(L("settings.cancel")) { withAnimation(motion) { deleting = nil } }
                                    }.frame(minHeight: 44)
                                }.transition(.opacity)
                            }
                            Divider()
                        }
                    }
                }
            }.frame(height: min(CGFloat(library.classification.categories.count) * 48 + (deleting == nil ? 0 : 100), 180))
        }
    }
    private func beginNaming(_ category: BookCategory?) {
        withAnimation(motion) { editing = category; name = category?.name ?? ""; naming = true; deleting = nil }
    }
    private func saveName() {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        change(["action": editing == nil ? "create" : "rename", "id": editing?.id ?? "", "name": name]) { naming = false; nameFocused = false }
    }
    private func filterChip(_ title: String, id: String?) -> some View {
        Button { withAnimation(motion) { filter = id } } label: {
            chip(title, selected: filter == id, selection: "category-filter-selection")
        }.buttonStyle(.plain).accessibilityAddTraits(filter == id ? [.isSelected] : [])
    }
    private func choice(_ title: String, id: String, ids: [String]) -> some View {
        Button { change(["action":"assign", "id":id, "books":ids]) { targets = nil } } label: {
            chip(title, selected: common == id, selection: "category-assignment-selection")
        }.buttonStyle(.plain).accessibilityAddTraits(common == id ? [.isSelected] : [])
    }
    private func chip(_ title: String, selected: Bool, selection: String) -> some View {
        HStack(spacing: 4) {
            if selected { Image(systemName: "checkmark").font(.caption).transition(.opacity) }
            Text(title.count > 14 ? String(title.prefix(14)) + "…" : title).font(.subheadline).lineLimit(1)
        }.padding(.horizontal, 12).frame(minHeight: 44)
            .background {
                if selected {
                    Capsule().fill(Color.accentColor.opacity(reduceTransparency ? 0.22 : 0.16))
                        .matchedGeometryEffect(id: selection, in: selectionNamespace)
                }
            }
            .contentShape(Capsule()).accessibilityLabel(title)
    }
    @ViewBuilder private func surface<Content: View, S: Shape>(_ content: Content, shape: S, id: String, interactive: Bool = false) -> some View {
        if reduceTransparency {
            content.background(Color(uiColor: .secondarySystemGroupedBackground), in: shape)
        } else if #available(iOS 26.0, *) {
            content.readerChromeGlass(in: shape, id: id, namespace: glassNamespace, interactive: interactive)
        } else {
            content.background(.regularMaterial, in: shape)
        }
    }
}
