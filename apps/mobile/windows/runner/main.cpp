#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <windows.h>

#include "app_links/app_links_plugin_c_api.h"
#include "flutter_window.h"
#include "identity_lease.h"
#include "utils.h"

int APIENTRY wWinMain(_In_ HINSTANCE instance, _In_opt_ HINSTANCE prev,
                      _In_ wchar_t *command_line, _In_ int show_command) {
  // Protect the shared identity even when two launches race before a window
  // exists. Keep the mutex until process exit; Windows releases it on crashes.
  HANDLE instance_mutex = CreateIdentityMutex();
  if (instance_mutex == nullptr) return EXIT_FAILURE;
  DWORD ownership = ::WaitForSingleObject(instance_mutex, 0);
  if (ownership == WAIT_FAILED) {
    ::CloseHandle(instance_mutex);
    return EXIT_FAILURE;
  }
  if (ownership != WAIT_OBJECT_0 && ownership != WAIT_ABANDONED) {
    for (int attempt = 0; attempt < 50; ++attempt) {
      if (SendAppLinkToInstance()) {
        ::CloseHandle(instance_mutex);
        return EXIT_SUCCESS;
      }
      ::Sleep(100);
    }
    ::MessageBoxW(nullptr, L"Omochi is already running.", L"Omochi",
                  MB_OK | MB_ICONINFORMATION);
    ::CloseHandle(instance_mutex);
    return EXIT_FAILURE;
  }
  // Attach to console when present (e.g., 'flutter run') or create a
  // new console when running with a debugger.
  if (!::AttachConsole(ATTACH_PARENT_PROCESS) && ::IsDebuggerPresent()) {
    CreateAndAttachConsole();
  }

  // Initialize COM, so that it is available for use in the library and/or
  // plugins.
  ::CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

  flutter::DartProject project(L"data");

  std::vector<std::string> command_line_arguments =
      GetCommandLineArguments();

  project.set_dart_entrypoint_arguments(std::move(command_line_arguments));

  FlutterWindow window(project);
  Win32Window::Point origin(10, 10);
  Win32Window::Size size(1280, 720);
  if (!window.Create(L"Omochi", origin, size)) {
    return EXIT_FAILURE;
  }
  window.SetQuitOnClose(true);

  ::MSG msg;
  while (::GetMessage(&msg, nullptr, 0, 0)) {
    ::TranslateMessage(&msg);
    ::DispatchMessage(&msg);
  }

  ::CoUninitialize();
  ::ReleaseMutex(instance_mutex);
  ::CloseHandle(instance_mutex);
  return EXIT_SUCCESS;
}
