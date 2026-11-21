!define LINK_SELECTOR_APP "Link Selector"
!define LINK_SELECTOR_CAPABILITIES "Software\Link Selector\Capabilities"
!define LINK_SELECTOR_PROGID "LinkSelector.Url"

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\RegisteredApplications" "${LINK_SELECTOR_APP}" "${LINK_SELECTOR_CAPABILITIES}"
  WriteRegStr HKCU "${LINK_SELECTOR_CAPABILITIES}" "ApplicationName" "${LINK_SELECTOR_APP}"
  WriteRegStr HKCU "${LINK_SELECTOR_CAPABILITIES}" "ApplicationDescription" "Choose a browser when opening a web link."
  WriteRegStr HKCU "${LINK_SELECTOR_CAPABILITIES}" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "${LINK_SELECTOR_CAPABILITIES}\URLAssociations" "http" "${LINK_SELECTOR_PROGID}"
  WriteRegStr HKCU "${LINK_SELECTOR_CAPABILITIES}\URLAssociations" "https" "${LINK_SELECTOR_PROGID}"
  WriteRegStr HKCU "Software\Classes\${LINK_SELECTOR_PROGID}" "" "Link Selector web link"
  WriteRegStr HKCU "Software\Classes\${LINK_SELECTOR_PROGID}" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\${LINK_SELECTOR_PROGID}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${LINK_SELECTOR_PROGID}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  System::Call 'shell32::SHChangeNotify(i 0x8000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\RegisteredApplications" "${LINK_SELECTOR_APP}"
  DeleteRegKey HKCU "${LINK_SELECTOR_CAPABILITIES}"
  DeleteRegKey /ifempty HKCU "Software\Link Selector"
  DeleteRegKey HKCU "Software\Classes\${LINK_SELECTOR_PROGID}"
  System::Call 'shell32::SHChangeNotify(i 0x8000000, i 0, p 0, p 0)'
!macroend
