import Cocoa
import CryptoKit
import Darwin
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    FlutterMethodChannel(
      name: "omochi/identity-lease",
      binaryMessenger: flutterViewController.engine.binaryMessenger
    ).setMethodCallHandler { call, result in
      do {
        guard let value = call.arguments as? String else {
          result(
            FlutterError(code: "identity-lease", message: "Invalid lease argument", details: nil))
          return
        }
        switch call.method {
        case "acquire":
          result(try IdentityLeases.acquire(namespace: value))
        case "release":
          IdentityLeases.release(token: value)
          result(nil)
        default:
          result(FlutterMethodNotImplemented)
        }
      } catch {
        result(
          FlutterError(code: "identity-lease", message: error.localizedDescription, details: nil))
      }
    }

    super.awakeFromNib()
  }
}

// Method handlers run on the platform thread. Each descriptor owns a flock,
// including across engines/isolates. Never unlink these files: that would let
// another process lock a different inode for the same identity.
private enum IdentityLeases {
  private static var descriptors: [String: Int32] = [:]

  static func acquire(namespace: String) throws -> String {
    let support = try FileManager.default.url(
      for: .applicationSupportDirectory, in: .userDomainMask,
      appropriateFor: nil, create: true)
    let directory = support.appendingPathComponent("omochi/identity-leases", isDirectory: true)
    try FileManager.default.createDirectory(
      at: directory, withIntermediateDirectories: true,
      attributes: [.posixPermissions: 0o700])
    let name = SHA256.hash(data: Data(namespace.utf8)).map { String(format: "%02x", $0) }.joined()
    let path = directory.appendingPathComponent(name + ".lock").path
    let fd = open(path, O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW, 0o600)
    guard fd >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
      let code = errno
      close(fd)
      if code == EWOULDBLOCK {
        throw NSError(
          domain: "OmochiIdentityLease", code: Int(code),
          userInfo: [NSLocalizedDescriptionKey: "This Hub is already running"])
      }
      throw NSError(domain: NSPOSIXErrorDomain, code: Int(code))
    }
    let token = UUID().uuidString
    descriptors[token] = fd
    return token
  }

  static func release(token: String) {
    if let fd = descriptors.removeValue(forKey: token) { close(fd) }
  }
}
