; Removes the per-user integrations Windows AI registered at runtime.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\*\shell\WindowsAI"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\WindowsAI"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.windowsai.bridge"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.windowsai.bridge"
  RMDir /r "$LOCALAPPDATA\WindowsAI\native-host"
  RMDir /r "$LOCALAPPDATA\WindowsAI\runs"
  Delete "$LOCALAPPDATA\WindowsAI\bridge.token"
!macroend
