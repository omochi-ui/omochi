import XCTest

@available(iOS 16.4, *)
final class LinkTests: XCTestCase {
  private func invitation(_ id: String) throws -> URL {
    let object: [String: Any] = [
      "version": 4, "address": ["id": String(repeating: id, count: 64), "addrs": []],
      "secret": String(repeating: "b", count: 64), "expires": 4_102_444_800,
    ]
    let data = try JSONSerialization.data(withJSONObject: object)
    let payload = data.base64EncodedString().replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    return URL(string: "omochi://invite/" + payload)!
  }
  func testColdAndWarmGatewayInvitation() throws {
    addUIInterruptionMonitor(withDescription: "Open Omochi invitation") { alert in
      if alert.buttons["Open"].exists {
        alert.buttons["Open"].tap()
        return true
      }
      return false
    }
    let app = XCUIApplication(bundleIdentifier: "fyi.omochi.mobile")
    app.terminate()
    app.open(try invitation("a"))
    XCTAssertTrue(app.buttons["Add to this device"].waitForExistence(timeout: 30))
    XCTAssertTrue(
      app.descendants(matching: .any).matching(
        NSPredicate(format: "value == %@", String(repeating: "a", count: 64))
      ).firstMatch.exists)
    app.buttons["Cancel"].tap()
    XCTAssertFalse(app.buttons["Add to this device"].exists)
    app.open(try invitation("c"))
    XCTAssertTrue(app.buttons["Add to this device"].waitForExistence(timeout: 15))
    XCTAssertTrue(
      app.descendants(matching: .any).matching(
        NSPredicate(format: "value == %@", String(repeating: "c", count: 64))
      ).firstMatch.exists)
    XCTAssertEqual(app.buttons.matching(identifier: "Add to this device").count, 1)
    app.buttons["Cancel"].tap()
  }
}
