// From a Visual Studio developer shell:
// cl /EHsc /std:c++17 identity_lease_test.cpp ../../windows/runner/identity_lease.cpp advapi32.lib
// Run identity_lease_test.exe with no Omochi instance running.
// Cross-session check: hold Omochi open, then launch it in another login session
// of the same Windows account; the second Hub must not start.
#include "../../windows/runner/identity_lease.h"

#include <sddl.h>

#include <iostream>
#include <string>
#include <vector>

int Probe(const wchar_t* mode) {
  wchar_t executable[MAX_PATH];
  if (!GetModuleFileNameW(nullptr, executable, MAX_PATH)) return -1;
  std::wstring command = L"\"" + std::wstring(executable) + L"\" " + mode;
  STARTUPINFOW startup = {};
  startup.cb = sizeof(startup);
  PROCESS_INFORMATION process = {};
  if (!CreateProcessW(nullptr, command.data(), nullptr, nullptr, FALSE, 0,
                      nullptr, nullptr, &startup, &process)) return -1;
  CloseHandle(process.hThread);
  if (WaitForSingleObject(process.hProcess, 10000) != WAIT_OBJECT_0) {
    TerminateProcess(process.hProcess, 99);
    CloseHandle(process.hProcess);
    return -1;
  }
  DWORD code = 0;
  GetExitCodeProcess(process.hProcess, &code);
  CloseHandle(process.hProcess);
  return static_cast<int>(code);
}

int main(int argc, char** argv) {
  HANDLE mutex = CreateIdentityMutex();
  if (!mutex) return 1;
  if (argc > 1) {
    const DWORD result = WaitForSingleObject(mutex, 0);
    if (result == WAIT_TIMEOUT) return 2;
    if (result != WAIT_OBJECT_0 && result != WAIT_ABANDONED) return 3;
    if (std::string(argv[1]) == "abandon") {
      // Exit while owning it, exercising Windows' crash recovery.
      ExitProcess(0);
    }
    ReleaseMutex(mutex);
    CloseHandle(mutex);
    return 0;
  }
  if (WaitForSingleObject(mutex, 0) != WAIT_OBJECT_0) return 4;

  // Verify the object is in Global and keyed by account SID, not session ID.
  HANDLE token = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return 5;
  DWORD size = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &size);
  std::vector<BYTE> buffer(size);
  if (!GetTokenInformation(token, TokenUser, buffer.data(), size, &size)) return 6;
  CloseHandle(token);
  LPWSTR sid = nullptr;
  if (!ConvertSidToStringSidW(
          reinterpret_cast<TOKEN_USER*>(buffer.data())->User.Sid, &sid)) return 7;
  const std::wstring name = std::wstring(L"Global\\fyi.omochi.mobile.") + sid;
  LocalFree(sid);
  HANDLE global = OpenMutexW(SYNCHRONIZE, FALSE, name.c_str());
  if (!global) return 8;
  CloseHandle(global);

  if (Probe(L"probe") != 2) return 9;
  ReleaseMutex(mutex);
  if (Probe(L"probe") != 0) return 10;
  if (Probe(L"abandon") != 0) return 11;
  if (WaitForSingleObject(mutex, 0) != WAIT_ABANDONED) return 12;
  ReleaseMutex(mutex);
  CloseHandle(mutex);
  std::cout << "PASS: Global user mutex, process exclusion, release and abandonment\n";
  return 0;
}
