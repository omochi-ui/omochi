#include "identity_lease.h"

#include <sddl.h>

#include <string>
#include <vector>

HANDLE CreateIdentityMutex() {
  HANDLE token = nullptr;
  if (!::OpenProcessToken(::GetCurrentProcess(), TOKEN_QUERY, &token)) {
    return nullptr;
  }
  DWORD size = 0;
  ::GetTokenInformation(token, TokenUser, nullptr, 0, &size);
  if (size == 0) {
    ::CloseHandle(token);
    return nullptr;
  }
  std::vector<BYTE> buffer(size);
  const BOOL read =
      ::GetTokenInformation(token, TokenUser, buffer.data(), size, &size);
  ::CloseHandle(token);
  if (!read) return nullptr;

  LPWSTR sid = nullptr;
  if (!::ConvertSidToStringSidW(
          reinterpret_cast<TOKEN_USER*>(buffer.data())->User.Sid, &sid)) {
    return nullptr;
  }
  // Secure storage is user-scoped, not login-session-scoped. Use the account SID
  // (not the logon SID), both in the global name and in the access descriptor.
  const std::wstring name = std::wstring(L"Global\\fyi.omochi.mobile.") + sid;
  const std::wstring dacl = std::wstring(L"D:P(A;;GA;;;") + sid + L")";
  ::LocalFree(sid);

  PSECURITY_DESCRIPTOR descriptor = nullptr;
  if (!::ConvertStringSecurityDescriptorToSecurityDescriptorW(
          dacl.c_str(), SDDL_REVISION_1, &descriptor, nullptr)) {
    return nullptr;
  }
  SECURITY_ATTRIBUTES attributes = {sizeof(SECURITY_ATTRIBUTES), descriptor,
                                    FALSE};
  HANDLE mutex = ::CreateMutexW(&attributes, FALSE, name.c_str());
  ::LocalFree(descriptor);
  return mutex;
}
