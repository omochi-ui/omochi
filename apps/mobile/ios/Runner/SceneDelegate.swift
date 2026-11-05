import Flutter
import UIKit
import app_links

class SceneDelegate: FlutterSceneDelegate {
  override func scene(
    _ scene: UIScene,
    willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions
  ) {
    super.scene(scene, willConnectTo: session, options: connectionOptions)
    forwardInvitations(connectionOptions.urlContexts)
  }

  override func scene(_ scene: UIScene, openURLContexts contexts: Set<UIOpenURLContext>) {
    if contexts.allSatisfy({ isInvitation($0.url) }) {
      forwardInvitations(contexts)
    } else {
      super.scene(scene, openURLContexts: contexts)
    }
  }

  private func isInvitation(_ url: URL) -> Bool {
    url.scheme?.lowercased() == "omochi"
      && ["invite", "share", "owner"].contains(url.host?.lowercased() ?? "")
  }

  private func forwardInvitations(_ contexts: Set<UIOpenURLContext>) {
    for context in contexts where isInvitation(context.url) {
      AppLinks.shared.handleLink(url: context.url)
    }
  }
}
