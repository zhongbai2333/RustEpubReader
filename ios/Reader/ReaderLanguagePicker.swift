import SwiftUI

struct ReaderLanguagePicker: View {
    @ObservedObject private var readerI18n=ReaderI18n.shared
    @Environment(\.dismiss) private var dismiss
    var body:some View {
        NavigationStack {
            List {
                ForEach(readerI18n.choices,id:\.code) {choice in
                    Button {
                        readerI18n.setLanguage(choice.code)
                        dismiss()
                    } label: {
                        HStack {
                            Text(choice.title).foregroundStyle(.primary)
                            Spacer()
                            if readerI18n.preference==choice.code {Image(systemName:"checkmark").foregroundStyle(.tint)}
                        }.frame(minHeight:32).contentShape(Rectangle())
                    }.accessibilityAddTraits(readerI18n.preference==choice.code ? .isSelected : [])
                }
            }.navigationTitle(L("language.title")).navigationBarTitleDisplayMode(.inline)
                .toolbar {ToolbarItem(placement:.confirmationAction) {Button(L("common.close")) {dismiss()}}}
        }.environment(\.locale,readerI18n.locale)
            .presentationDetents([.medium,.large]).presentationDragIndicator(.visible)
    }
}
