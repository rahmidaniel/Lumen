; Hooks for Tauri's NSIS installer (bundle.windows.nsis.installerHooks).
;
; When Lumen starts OpenRGB itself, OpenRGB runs from $INSTDIR\openrgb, and it
; keeps running after Lumen quits to install an update. Its files are about to
; be replaced or removed, so the installer would stop with "Error opening file
; for writing". End it first; Lumen starts it again. The Restart Manager goes by
; file path, so an OpenRGB installed elsewhere and the copy Lumen's service runs
; from Program Files are left alone. A copy this installer can't end (one running
; elevated) falls through to the usual Abort/Retry/Ignore prompt.

!macro LUMEN_END_OPENRGB
  Push $0
  Push $1
  ; Lumen first, or it would start OpenRGB again straight away. This is the
  ; check the installer runs next anyway, with the same prompt.
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  ${If} ${FileExists} "$INSTDIR\openrgb\OpenRGB.exe"
    !insertmacro RestartManager_ShutdownFile "$INSTDIR\openrgb\OpenRGB.exe" $0
    ; A second or two after a program exits, Windows' Inventory and Compatibility
    ; Appraisal service opens its file for a moment without sharing write access,
    ; and a copy that lands in that moment fails. It does share delete access, so
    ; move the file out of the way: the copy then writes a new one.
    Delete "$INSTDIR\openrgb\OpenRGB.exe.old"
    StrCpy $1 0
    ${Do}
      ClearErrors
      Rename "$INSTDIR\openrgb\OpenRGB.exe" "$INSTDIR\openrgb\OpenRGB.exe.old"
      ${IfNot} ${Errors}
        ${Break}
      ${EndIf}
      Sleep 100
      IntOp $1 $1 + 1
    ${LoopUntil} $1 >= 50
    Delete "$INSTDIR\openrgb\OpenRGB.exe.old"
  ${EndIf}
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro LUMEN_END_OPENRGB
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro LUMEN_END_OPENRGB
!macroend
