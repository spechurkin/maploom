; User files in $INSTDIR\themes are NEVER installer resources.
; Tauri's NSIS template deletes listed resources and uses non-recursive RMDir
; on $INSTDIR. Keep the directory even when no themes have been installed yet.
!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$INSTDIR\themes"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  CreateDirectory "$INSTDIR\themes"
!macroend
