param(
  [string]$InstallerScript = "",
  [string]$OutputPath = "",
  [string]$WebUninstallerPath = $env:MYLIST_WEB_UNINSTALLER
)
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($InstallerScript)) {
  $InstallerScript = Join-Path $PSScriptRoot '..\src-tauri\target\release\nsis\x64\installer.nsi'
}
if (!(Test-Path -LiteralPath $InstallerScript)) { throw "Installer script not found: $InstallerScript" }
$uninstallIcon = Join-Path $PSScriptRoot '..\src-tauri\icons\uninstall.ico'
if (!(Test-Path -LiteralPath $uninstallIcon)) { throw "Uninstaller icon not found: $uninstallIcon" }
$uninstallIcon = (Resolve-Path -LiteralPath $uninstallIcon).Path
$text = Get-Content -LiteralPath $InstallerScript -Raw -Encoding UTF8
$installerDirectory = Split-Path -Parent $InstallerScript
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
  $versionMatch = [regex]::Match($text, '(?m)^!define VERSION "([^"]+)"')
  if (!$versionMatch.Success) { throw 'Could not locate generated installer version.' }
  $OutputPath = Join-Path (Split-Path -Parent (Split-Path -Parent $installerDirectory)) "bundle\nsis\MyLIST_$($versionMatch.Groups[1].Value)_x64-setup.exe"
}
$localeSourceDirectory = Join-Path $PSScriptRoot '..\src-tauri\nsis\locales'
$localeFileNames = @(
  'English.nsh', 'German.nsh', 'French.nsh', 'Italian.nsh',
  'Spanish.nsh', 'Japanese.nsh', 'SimpChinese.nsh', 'TradChinese.nsh'
)
foreach ($localeFileName in $localeFileNames) {
  $sourcePath = Join-Path $localeSourceDirectory $localeFileName
  if (!(Test-Path -LiteralPath $sourcePath)) { throw "Installer locale not found: $sourcePath" }
  Copy-Item -LiteralPath $sourcePath -Destination (Join-Path $installerDirectory $localeFileName) -Force
}

# Use the dedicated uninstall mark for the standalone uninstaller without
# changing the installer or application icon.
$text = $text.Replace('!define UNINSTALLERICON ""', "!define UNINSTALLERICON `"$uninstallIcon`"")

if (![string]::IsNullOrWhiteSpace($WebUninstallerPath)) {
  if (!(Test-Path -LiteralPath $WebUninstallerPath)) { throw "Web uninstaller not found: $WebUninstallerPath" }
  $WebUninstallerPath = (Resolve-Path -LiteralPath $WebUninstallerPath).Path
  $createNeedle = '  WriteUninstaller "$INSTDIR\uninstall.exe"'
  $createReplacement = "  CreateDirectory `"`$INSTDIR\.mylist`"`r`n  WriteUninstaller `"`$INSTDIR\.mylist\uninstall-core.exe`"`r`n  SetFileAttributes `"`$INSTDIR\.mylist`" HIDDEN`r`n  Delete `"`$INSTDIR\uninstall-core.exe`"`r`n  Delete `"`$INSTDIR\uninstall-ui.exe`"`r`n  File /a `"/oname=uninstall.exe`" `"$WebUninstallerPath`""
  if (!$text.Contains($createNeedle)) { throw 'Could not locate WriteUninstaller.' }
  $text = $text.Replace($createNeedle, $createReplacement)
  $text = $text.Replace('  Delete "$INSTDIR\uninstall.exe"', "  Delete `"`$INSTDIR\.mylist\uninstall-core.exe`"`r`n  RMDir `"`$INSTDIR\.mylist`"`r`n  Delete `"`$INSTDIR\uninstall-core.exe`"`r`n  Delete /REBOOTOK `"`$INSTDIR\uninstall.exe`"`r`n  Delete /REBOOTOK `"`$INSTDIR\uninstall-ui.exe`"")
  $text = $text.Replace('${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0"', '${GetSize} "$INSTDIR" "/M=uninstall-core.exe /S=0K /G=0"')
}

# Include all supported installer/uninstaller languages. The generated NSIS
# project includes English only; the remaining language resources live beside
# this script and are copied into the generated NSIS directory above.
$languageIncludeBlock = @"
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "Italian"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "Japanese"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "TradChinese"
!insertmacro MUI_RESERVEFILE_LANGDLL
  !include "$installerDirectory\English.nsh"
  !include "$installerDirectory\German.nsh"
  !include "$installerDirectory\French.nsh"
  !include "$installerDirectory\Italian.nsh"
  !include "$installerDirectory\Spanish.nsh"
  !include "$installerDirectory\Japanese.nsh"
  !include "$installerDirectory\SimpChinese.nsh"
  !include "$installerDirectory\TradChinese.nsh"
"@
$languageIncludePattern = '(?ms)^!insertmacro MUI_LANGUAGE "English"\r?\n!insertmacro MUI_RESERVEFILE_LANGDLL\r?\n\s*!include "[^"]*English\.nsh"\r?\n'
if ($text -notmatch $languageIncludePattern) { throw 'Could not locate generated installer language include block.' }
$text = [regex]::Replace($text, $languageIncludePattern, { param($match) $languageIncludeBlock }, 1)

# Remove the legacy reinstall/uninstall choice page. Existing versions are handled in .onInit.
$text = [regex]::Replace($text, '(?s); 4\. Custom page to ask user if he wants to reinstall/uninstall.*?; 5\. Choose install directory page', "; 4. Existing installations are handled automatically in .onInit.
; The legacy maintenance radio page is intentionally omitted.

; 5. Choose install directory page")

# Omit the generic welcome page; the directory page is the first interactive page.
$text = [regex]::Replace($text, '(?m)^!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\r?\n!insertmacro MUI_PAGE_WELCOME\r?\n', "; Welcome page intentionally omitted.`r`n")
# Internal uninstall during upgrade must skip its own confirmation page.
$hookOld = @'
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd
'@
$hookNew = @'
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
  ${IfThen} $UpdateMode = 1  ${|} Abort ${|}
FunctionEnd
'@
$text = $text.Replace($hookOld, $hookNew)

# Apply a light, minimal MUI surface to the standard NSIS pages.
$anchor = '!include MUI2.nsh'
$theme = "$anchor
!define MUI_BGCOLOR F7FAFE
!define MUI_TEXTCOLOR 1A2230
!define MUI_HEADERCOLOR F7FAFE
"
if ($text.Contains($anchor) -and !$text.Contains('!define MUI_BGCOLOR')) { $text = $text.Replace($anchor, $theme) }

# Confirm once, then always uninstall the previous version before continuing.
$oldInit = '(?s)Function \.onInit.*?FunctionEnd\r?\n\s*Section EarlyChecks'
$newInit = @'
Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}
  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}
  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
  Call SelectSystemLanguage
  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif
  !insertmacro SetContext
  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    Call RestorePreviousInstallLocation
  ${EndIf}
FunctionEnd

Section EarlyChecks
'@
if ($text -notmatch $oldInit) { throw 'Could not locate generated .onInit block.' }
$text = [regex]::Replace($text, $oldInit, { param($match) $newInit })

# Map the Windows display language to the eight MyLIST installer languages.
# Unrecognised locales intentionally fall back to English.
$systemLanguageFunctions = @'
Function SelectSystemLanguage
  StrCpy $LANGUAGE ${LANG_ENGLISH}
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  ${If} $0 == 1031
    StrCpy $LANGUAGE ${LANG_GERMAN}
  ${ElseIf} $0 == 1036
    StrCpy $LANGUAGE ${LANG_FRENCH}
  ${ElseIf} $0 == 1040
    StrCpy $LANGUAGE ${LANG_ITALIAN}
  ${ElseIf} $0 == 1034
    StrCpy $LANGUAGE ${LANG_SPANISH}
  ${ElseIf} $0 == 1041
    StrCpy $LANGUAGE ${LANG_JAPANESE}
  ${ElseIf} $0 == 2052
    StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
  ${ElseIf} $0 == 1028
    StrCpy $LANGUAGE ${LANG_TRADCHINESE}
  ${ElseIf} $0 == 3076
    StrCpy $LANGUAGE ${LANG_TRADCHINESE}
  ${EndIf}
FunctionEnd

Function un.SelectSystemLanguage
  StrCpy $LANGUAGE ${LANG_ENGLISH}
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  ${If} $0 == 1031
    StrCpy $LANGUAGE ${LANG_GERMAN}
  ${ElseIf} $0 == 1036
    StrCpy $LANGUAGE ${LANG_FRENCH}
  ${ElseIf} $0 == 1040
    StrCpy $LANGUAGE ${LANG_ITALIAN}
  ${ElseIf} $0 == 1034
    StrCpy $LANGUAGE ${LANG_SPANISH}
  ${ElseIf} $0 == 1041
    StrCpy $LANGUAGE ${LANG_JAPANESE}
  ${ElseIf} $0 == 2052
    StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
  ${ElseIf} $0 == 1028
    StrCpy $LANGUAGE ${LANG_TRADCHINESE}
  ${ElseIf} $0 == 3076
    StrCpy $LANGUAGE ${LANG_TRADCHINESE}
  ${EndIf}
FunctionEnd
'@
$unInitPattern = '(?s)Function un\.onInit\r?\n.*?\r?\nFunctionEnd'
if ($text -notmatch $unInitPattern) { throw 'Could not locate generated un.onInit block.' }
$unInitCallNeedle = '  !insertmacro MUI_UNGETLANGUAGE'
if (!$text.Contains($unInitCallNeedle)) { throw 'Could not locate MUI_UNGETLANGUAGE in un.onInit.' }
# MUI_UNGETLANGUAGE may restore a previously stored language. Select the current
# Windows display language after it so the standalone uninstaller follows the OS.
$text = $text.Replace($unInitCallNeedle, "$unInitCallNeedle`r`n  Call un.SelectSystemLanguage")
$deleteDataNeedle = '  Call un.SelectSystemLanguage'
$deleteDataHook = @'
  Call un.SelectSystemLanguage
  ${GetOptions} $CMDLINE "/WEBUI_DELETE_DATA" $0
  ${IfNot} ${Errors}
    StrCpy $DeleteAppDataCheckboxState 1
  ${EndIf}
'@
$text = $text.Replace($deleteDataNeedle, $deleteDataHook.TrimEnd())
$unInitIndex = $text.IndexOf('Function un.onInit')
if ($unInitIndex -lt 0) { throw 'Could not insert uninstaller system language selection.' }
$text = $text.Insert($unInitIndex, "$systemLanguageFunctions`r`n")
# Let the Modern UI advance directly from 100% installation progress to the
# finish page instead of pausing on the completed InstFiles page.
$text = [regex]::Replace($text, '(?m)^!define MUI_FINISHPAGE_NOAUTOCLOSE\r?\n', '; Installation progress advances automatically to the finish page.`r`n')
Set-Content -LiteralPath $InstallerScript -Value $text -Encoding UTF8 -NoNewline

$makensis = Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'
if (!(Test-Path -LiteralPath $makensis)) { throw "makensis not found: $makensis" }
Push-Location (Split-Path -Parent $InstallerScript)
try { & $makensis /V2 /NOCD (Split-Path -Leaf $InstallerScript) } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw "makensis failed with exit code $LASTEXITCODE" }
$built = Join-Path (Split-Path $InstallerScript) 'nsis-output.exe'
if (!(Test-Path -LiteralPath $built)) { throw "NSIS output not found: $built" }
New-Item -ItemType Directory -Force -Path (Split-Path $OutputPath) | Out-Null
Copy-Item -LiteralPath $built -Destination $OutputPath -Force
Write-Output "Installer written: $OutputPath"
