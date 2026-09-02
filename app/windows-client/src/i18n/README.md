# MyLIST localization

This directory contains the source locale modules used by MyLIST.

## Files

- zh-CN.ts is the complete Simplified Chinese baseline.
- en.ts, de.ts, fr.ts, it.ts, es.ts, ja.ts, and zh-TW.ts contain the other supported languages.
- index.ts defines locale types, formatting, validation, and runtime switching.

## Editable runtime locale files

When external locale files are enabled, MyLIST creates editable copies for the current Windows user:

    %LOCALAPPDATA%\com.mylist.desktop\locales\

Existing editable files are never overwritten automatically. A missing, unreadable, or incomplete locale safely falls back to English.

## Translation rules

- Edit message values only. Do not change message keys, interpolation names such as {count}, or locale identifiers.
- Keep one message per line and preserve the module grouping used by zh-CN.ts.
- calendar.weekdays is the only list value. It must contain seven short labels separated by vertical bars, from Sunday through Saturday.
- Translate all eight stable default-category keys. User-created and user-renamed categories always preserve the text entered by the user.

Language selection is persisted locally. The app, tray menu, installer, uninstaller, and supported native labels should remain aligned with the selected locale.
