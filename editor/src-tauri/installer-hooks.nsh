; Installer hooks for Tauri's NSIS template.
;
; Tauri installs the editor, and bundle.resources in tauri.conf.json carries the
; daemon, run-hidden.vbs, the hook template and the shipped data beside it.
; This file adds what Tauri cannot know about:
;
;   * where DCS saves to, so the hook goes into the right Scripts\Hooks and the
;     programs build the catalogue from the right DCS-BIOS
;   * where profiles and the catalogue are written, which is not the install
;     folder, because each install replaces that
;   * the DCS hook itself, with DSC_DIR filled in, and its removal
;   * not starting at all while DCS, the daemon or the editor is running
;   * not replacing dcs-signal.exe while DCS has it running
;
; The two folders are recorded under HKCU\Software\DCS Signal Converter, which
; dsc-config's paths module reads. Keep the names in step with it.

!include TextFunc.nsh

!define DSC_KEY "Software\DCS Signal Converter"
!define DSC_DAEMON "dcs-signal.exe"
!define DSC_EDITOR "DCS Signal Converter.exe"
!define DSC_DCS "DCS.exe"
!define DSC_HOOK "dcs-signal-hook.lua"
!define DSC_FOLDER "DCS Signal Converter"
; FOLDERID_SavedGames
!define DSC_SAVED_GAMES_ID "{4C5C32FF-BB9D-43b0-B5B4-2D72E54EAAA4}"

Var DscSavedGames
Var DscDcsDir
Var DscDataDir

; The user's Saved Games folder, asked of Windows because users move it.
!macro DSC_SAVED_GAMES
  System::Call 'shell32::SHGetKnownFolderPath(g "${DSC_SAVED_GAMES_ID}", i 0, p 0, *p .r1) i .r2'
  ${If} $2 = 0
    System::Call '*$1(&w${NSIS_MAX_STRLEN} .s)'
    Pop $DscSavedGames
  ${Else}
    StrCpy $DscSavedGames "$PROFILE\Saved Games"
  ${EndIf}
  System::Call 'ole32::CoTaskMemFree(p r1)'
!macroend

; Before the first page, ask for DCS, the daemon and the editor to be closed.
;
; Asked up front rather than at the install step, so nobody clicks through
; every page to be told at the end, and so an update never lands under a
; running daemon: it reconciles profiles when it starts, and one already
; running goes on flying the old files until something makes it reload.
; Closing DCS is what closes the daemon, and a hook DCS has loaded is only
; replaced by a restart, so DCS is the one named first.
;
; Nothing is killed here. The checks in the install and uninstall sections
; still run, and they are the only ones a silent install reaches, since it
; has no GUI to initialise.
;
; Found with tasklist rather than nsis_tauri_utils: Tauri's template adds its
; plugin folder after including this file, and a function body is compiled
; where it stands. A match is a CSV row, which starts with a quote; no match
; is a message in the system's language, which never does.
!define MUI_CUSTOMFUNCTION_GUIINIT DscCheckRunning
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.DscCheckRunning

; $R0 is 1 when a process of this image name is running, else 0.
!macro DSC_IS_RUNNING image
  nsExec::ExecToStack 'tasklist /NH /FO CSV /FI "IMAGENAME eq ${image}"'
  Pop $R0
  Pop $R2
  StrCpy $R2 $R2 1
  ${If} $R2 == '"'
    StrCpy $R0 1
  ${Else}
    StrCpy $R0 0
  ${EndIf}
!macroend

!macro DSC_CHECK_RUNNING
  dsc_running_check:
  StrCpy $R1 ""
  !insertmacro DSC_IS_RUNNING "${DSC_DCS}"
  ${If} $R0 = 1
    StrCpy $R1 "$R1$\r$\n    DCS World"
  ${EndIf}
  !insertmacro DSC_IS_RUNNING "${DSC_DAEMON}"
  ${If} $R0 = 1
    StrCpy $R1 "$R1$\r$\n    DCS Signal Converter (running with DCS)"
  ${EndIf}
  !insertmacro DSC_IS_RUNNING "${DSC_EDITOR}"
  ${If} $R0 = 1
    StrCpy $R1 "$R1$\r$\n    DCS Signal Converter editor"
  ${EndIf}
  ${If} $R1 != ""
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Close these before continuing:$\r$\n$R1$\r$\n$\r$\nThen choose Retry." IDRETRY dsc_running_check
    Quit
  ${EndIf}
!macroend

Function DscCheckRunning
  !insertmacro DSC_CHECK_RUNNING
FunctionEnd

Function un.DscCheckRunning
  !insertmacro DSC_CHECK_RUNNING
FunctionEnd

; Wait for the daemon to go before its files are replaced or removed. It runs
; while DCS does, and killing it would leave whatever it lit on the panels.
; A silent install has no one to ask, so it stops it.
;
; Inserted once in the install section and once in the uninstall section, so
; its labels are never seen twice in one scope.
!macro DSC_WAIT_FOR_DAEMON
  dsc_daemon_check:
  nsis_tauri_utils::FindProcess "${DSC_DAEMON}"
  Pop $R0
  ${If} $R0 = 0
    ${If} ${Silent}
      nsis_tauri_utils::KillProcess "${DSC_DAEMON}"
      Pop $R0
    ${Else}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "DCS Signal Converter is running, most likely because DCS World is.$\r$\n$\r$\nClose DCS World, then choose Retry." IDRETRY dsc_daemon_check
      Abort
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro DSC_WAIT_FOR_DAEMON

  ; Clear what an earlier install shipped, so installing over it without
  ; uninstalling first leaves nothing behind that this release dropped: a
  ; retired default in data\defaults would otherwise go on being seeded.
  ; Only shipped files live here; everything written is under DataDir.
  ; The folders go whole. Top-level files are named, since the install folder
  ; may be one the user chose with other things in it; when a release stops
  ; shipping a top-level file, keep its name in this list.
  RMDir /r "$INSTDIR\data"
  RMDir /r "$INSTDIR\hook"
  Delete "$INSTDIR\${DSC_DAEMON}"
  Delete "$INSTDIR\run-hidden.vbs"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DSC_SAVED_GAMES

  ; An earlier install already asked. Keep its answers while the DCS folder
  ; is still there, so an update never asks again.
  ReadRegStr $DscDcsDir HKCU "${DSC_KEY}" "DcsDir"
  ReadRegStr $DscDataDir HKCU "${DSC_KEY}" "DataDir"
  ${If} $DscDcsDir == ""
  ${OrIf} $DscDataDir == ""
  ${OrIfNot} ${FileExists} "$DscDcsDir\*.*"
    Call DscChooseFolders
  ${EndIf}

  WriteRegStr HKCU "${DSC_KEY}" "DcsDir" "$DscDcsDir"
  WriteRegStr HKCU "${DSC_KEY}" "DataDir" "$DscDataDir"
  CreateDirectory "$DscDataDir"
  DetailPrint "DCS saves to $DscDcsDir"
  DetailPrint "Profiles and catalogue in $DscDataDir"

  ; The hook, from the template, pointed at this install.
  CreateDirectory "$DscDcsDir\Scripts\Hooks"
  ClearErrors
  FileOpen $3 "$INSTDIR\hook\${DSC_HOOK}" r
  FileOpen $4 "$DscDcsDir\Scripts\Hooks\${DSC_HOOK}" w
  ${IfNot} ${Errors}
    dsc_hook_line:
      FileRead $3 $5
      IfErrors dsc_hook_done
      nsis_tauri_utils::StrReplace "$5" "DSC_DIR" "$INSTDIR"
      Pop $5
      FileWrite $4 $5
      Goto dsc_hook_line
    dsc_hook_done:
    DetailPrint "DCS hook installed in $DscDcsDir\Scripts\Hooks"
    ; DCS loads hooks once, at launch, so a running DCS keeps the old one.
    nsis_tauri_utils::FindProcess "DCS.exe"
    Pop $R0
    ${If} $R0 = 0
      DetailPrint "DCS World is running; it picks up the new hook when restarted"
      MessageBox MB_OK|MB_ICONINFORMATION "DCS World is running. Restart it so it picks up the updated DCS Signal Converter hook." /SD IDOK
    ${EndIf}
  ${Else}
    DetailPrint "Could not write the DCS hook to $DscDcsDir\Scripts\Hooks"
    MessageBox MB_OK|MB_ICONEXCLAMATION "Setup could not write the DCS hook to$\r$\n$DscDcsDir\Scripts\Hooks$\r$\n$\r$\nDCS Signal Converter will not start with DCS until it is there." /SD IDOK
  ${EndIf}
  FileClose $3
  FileClose $4
!macroend

; Where DCS saves, and where profiles go.
;
; DCS saves to Saved Games\DCS unless its install carries dcs_variant.txt,
; which makes it Saved Games\DCS.<variant>. Only the plain case, with a Config
; folder DCS has already made, is taken without asking. Anything else, a
; variant, a folder not found or DCS never run, and the user is asked for both
; folders rather than have either assumed. A silent install takes the best
; guess.
Function DscChooseFolders
  StrCpy $6 "DCS"
  StrCpy $7 ""
  ReadRegStr $1 HKCU "Software\Eagle Dynamics\DCS World" "Path"
  ${If} $1 != ""
  ${AndIf} ${FileExists} "$1\dcs_variant.txt"
    ClearErrors
    FileOpen $2 "$1\dcs_variant.txt" r
    ${IfNot} ${Errors}
      FileRead $2 $3
      FileClose $2
      ${TrimNewLines} "$3" $3
      ${If} $3 != ""
        StrCpy $6 "DCS.$3"
        StrCpy $7 "DCS World is set to save to a folder of its own, $6."
      ${EndIf}
    ${EndIf}
  ${EndIf}
  StrCpy $DscDcsDir "$DscSavedGames\$6"
  StrCpy $DscDataDir "$DscSavedGames\${DSC_FOLDER}"

  ${If} $7 == ""
    ${If} ${FileExists} "$DscDcsDir\Config\*.*"
      Return
    ${EndIf}
    StrCpy $7 "$DscDcsDir does not look like a folder DCS World has saved to yet."
  ${EndIf}
  ${If} ${Silent}
    Return
  ${EndIf}

  MessageBox MB_OK|MB_ICONINFORMATION "Setup could not confirm where DCS World saves its settings.$\r$\n$\r$\n$7$\r$\n$\r$\nNext, choose the DCS folder in Saved Games: the one holding Config and Scripts, where DCS-BIOS is installed."
  nsDialogs::SelectFolderDialog "Choose the DCS folder in Saved Games (it holds Config and Scripts)" "$DscDcsDir"
  Pop $0
  ${If} $0 != "error"
  ${AndIf} $0 != ""
    StrCpy $DscDcsDir $0
  ${EndIf}

  MessageBox MB_YESNO|MB_ICONQUESTION "Keep your DCS Signal Converter profiles in$\r$\n$DscDataDir?$\r$\n$\r$\nChoose No to pick another folder." IDYES dsc_data_done
  CreateDirectory "$DscDataDir"
  nsDialogs::SelectFolderDialog "Choose where DCS Signal Converter keeps your profiles" "$DscDataDir"
  Pop $0
  ${If} $0 != "error"
  ${AndIf} $0 != ""
    StrCpy $DscDataDir $0
  ${EndIf}
  dsc_data_done:
FunctionEnd

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro DSC_WAIT_FOR_DAEMON

  ReadRegStr $DscDcsDir HKCU "${DSC_KEY}" "DcsDir"
  ReadRegStr $DscDataDir HKCU "${DSC_KEY}" "DataDir"

  ; An update puts the hook straight back, so leave it.
  ${If} $UpdateMode <> 1
  ${AndIf} $DscDcsDir != ""
    Delete "$DscDcsDir\Scripts\Hooks\${DSC_HOOK}"
  ${EndIf}

  ; The catalogue is rebuilt from DCS-BIOS whenever it is missing, so it goes.
  ; Profiles are the user's work and stay, unless they ask for app data to go.
  ${If} $UpdateMode <> 1
  ${AndIf} $DscDataDir != ""
    RMDir /r "$DscDataDir\catalogue"
    RMDir /r "$DscDataDir\catalogue.building"
    RMDir /r "$DscDataDir\catalogue.old"
    Delete "$DscDataDir\catalogue.lock"
    ${If} $DeleteAppDataCheckboxState = 1
      RMDir /r "$DscDataDir\profiles"
      DeleteRegKey HKCU "${DSC_KEY}"
    ${EndIf}
    RMDir "$DscDataDir"
  ${EndIf}
!macroend
