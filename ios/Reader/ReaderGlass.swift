import SwiftUI
import UIKit

extension Color {
    static let readerAppTint = Color(red: 0.18, green: 0.39, blue: 0.75)
}

extension View {
    @ViewBuilder func readerChromeTitleGlass(namespace: Namespace.ID) -> some View {
        if #available(iOS 26.0, *) {
            readerChromeGlass(in: RoundedRectangle(cornerRadius: 16), id: "title", namespace: namespace, interactive: false)
        } else {
            readerGlass(in: RoundedRectangle(cornerRadius: 16)).transition(.opacity)
        }
    }
    @ViewBuilder func readerChromeGlass<S: Shape>(in shape: S, id: String, namespace: Namespace.ID, interactive: Bool = true) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular.interactive(interactive), in: shape)
                .glassEffectID(id, in: namespace)
                .glassEffectTransition(.materialize)
        } else {
            self.transition(.opacity)
        }
    }
    @ViewBuilder func readerGlass<S: Shape>(in shape: S, interactive: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular.interactive(interactive), in: shape)
        } else {
            background(.regularMaterial, in: shape)
        }
    }

    func readerPanelSurface(appearance: AppearanceCoordinator) -> some View {
        scrollContentBackground(.hidden)
            .toolbarBackground(.hidden, for: .navigationBar)
            .preferredColorScheme(appearance.appDark ? .dark : .light)
            .presentationBackground(.regularMaterial)
    }
}

// Keep each control separate even at rest. The system owns materialization;
// these surfaces must not morph into their neighbouring title or buttons.
struct ReaderChromeGlassContainer<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer(spacing: 0) { content }
        } else {
            content
        }
    }
}

// Keep native buttons above the reader's native text view so glass does not
// change gesture priority or let the page consume their taps.
@MainActor func configureReaderGlassButton(_ button: UIButton, nativeGlass: Bool = true) {
    if #available(iOS 26.0, *) {
        // The reader chrome uses a SwiftUI glass surface so its insertion and
        // removal can participate in the public materialize transition.
        var configuration = nativeGlass ? UIButton.Configuration.glass() : .plain()
        configuration.cornerStyle = .capsule
        configuration.contentInsets = .zero
        button.configuration = configuration
    } else {
        let material = UIVisualEffectView(effect: UIBlurEffect(style: .systemMaterial))
        material.isUserInteractionEnabled = false
        material.layer.cornerRadius = 22
        material.clipsToBounds = true
        material.translatesAutoresizingMaskIntoConstraints = false
        button.insertSubview(material, at: 0)
        NSLayoutConstraint.activate([
            material.leadingAnchor.constraint(equalTo: button.leadingAnchor),
            material.trailingAnchor.constraint(equalTo: button.trailingAnchor),
            material.topAnchor.constraint(equalTo: button.topAnchor),
            material.bottomAnchor.constraint(equalTo: button.bottomAnchor)
        ])
    }
}

@MainActor func updateReaderGlassButton(_ button: UIButton, symbol: String, size: CGFloat, tint: UIColor) {
    let image = UIImage(systemName: symbol)
    let symbols = UIImage.SymbolConfiguration(pointSize: size, weight: .medium)
    if var configuration = button.configuration {
        configuration.image = image
        configuration.preferredSymbolConfigurationForImage = symbols
        configuration.baseForegroundColor = tint
        button.configuration = configuration
    } else {
        button.setImage(image, for: .normal)
        button.setPreferredSymbolConfiguration(symbols, forImageIn: .normal)
    }
    button.tintColor = tint
}
