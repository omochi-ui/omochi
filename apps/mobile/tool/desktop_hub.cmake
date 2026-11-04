# Included after the runner and install directories have been defined.
# Run Cargo on every Flutter build; Cargo performs dependency tracking itself.
get_directory_property(HUB_FLUTTER_ROOT DIRECTORY "${FLUTTER_MANAGED_DIR}" DEFINITION FLUTTER_ROOT)
if(WIN32)
  set(HUB_DART "${HUB_FLUTTER_ROOT}/bin/cache/dart-sdk/bin/dart.exe")
  set(HUB_LIBRARY_NAME "omochi_native_bridge.dll")
else()
  set(HUB_DART "${HUB_FLUTTER_ROOT}/bin/cache/dart-sdk/bin/dart")
  set(HUB_LIBRARY_NAME "libomochi_native_bridge.so")
endif()
set(HUB_OUTPUT_DIR "${CMAKE_BINARY_DIR}/shared-hub/$<CONFIG>")
add_custom_target(omochi_native_hub
  COMMAND "${HUB_DART}" "${CMAKE_CURRENT_LIST_DIR}/build_desktop_hub.dart"
    --target "${FLUTTER_TARGET_PLATFORM}" --mode "$<CONFIG>"
    --output "${HUB_OUTPUT_DIR}"
  VERBATIM
)
add_dependencies(${BINARY_NAME} omochi_native_hub)
install(FILES "${HUB_OUTPUT_DIR}/${HUB_LIBRARY_NAME}"
  DESTINATION "${INSTALL_BUNDLE_LIB_DIR}" COMPONENT Runtime)
