import SwiftUI

@main
struct KeepsayApp: App {
    @State private var model = AppModel.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .onChange(of: scenePhase) { _, newPhase in
                    switch newPhase {
                    case .background:
                        // "離開後不保留": drop audio and results when leaving.
                        if case .recording = model.phase { model.cancel() }
                        if !model.phase.isBusy { model.dismiss() }
                    case .active:
                        model.refresh()
                    default:
                        break
                    }
                }
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        switch model.onboarding {
        case .key:
            KeyOnboardingView()
        case .model:
            ModelOnboardingView()
        case .finished:
            NavigationStack {
                HomeView()
            }
            .overlay(alignment: .bottom) { ToastView(message: model.toast) }
        }
    }
}

struct ToastView: View {
    let message: String?

    var body: some View {
        if let message {
            Text(message)
                .font(.subheadline)
                .padding(.horizontal, 16)
                .padding(.vertical, 9)
                .background(.thinMaterial, in: Capsule())
                .padding(.bottom, 24)
                .transition(.opacity)
                .accessibilityAddTraits(.updatesFrequently)
        }
    }
}
