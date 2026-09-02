# Install MyLIST MCP and Skill

MyLIST data stays on this computer. This guide registers a local MCP bridge and installs the MyLIST Skill for Codex. Configuration does not require MyLIST to be running, the Local MCP service to be enabled, or any interaction with the MyLIST interface.

## 1. Understand the stable location record

MyLIST stores its current installation directory at this fixed per-user location:

```text
%LOCALAPPDATA%\com.mylist.desktop\.install-location
```

The installer refreshes this record after every successful installation or relocation. The MCP configuration must resolve the executable through this record and must not contain a MyLIST installation path.

In the remaining steps, `<install-dir>` means the absolute directory stored in this location record. Confirm that it contains both `windows-client.exe` and the `docs` directory before copying any bundled file.

## 2. Protect the existing Codex configuration

Before changing MCP settings:

- Confirm that the existing Codex configuration loads successfully. If Codex reports a TOML parse error, stop and report it; do not attempt an MCP installation on a malformed file.
- Copy `~/.codex/config.toml` to a timestamped backup as raw bytes. Do not decode and rewrite the whole file.
- Never use default `Get-Content` / `Set-Content`, a regular-expression table replacement, or an editor that changes the file encoding. A UTF-8 file without a BOM can be corrupted by Windows PowerShell 5.1's default ANSI decoding, especially when project paths contain non-ASCII characters.
- Use the Codex MCP CLI in the next step. It owns the configuration update and preserves unrelated settings.

If any configuration command fails, restore the byte-for-byte backup before reporting the error.

## 3. Install the location-independent MCP configuration

Use the Codex MCP CLI to remove the old entry and add a new one. Do not manually edit `[mcp_servers.mylist]`.

Run the equivalent of these commands in the user's normal Codex environment:

```powershell
codex mcp remove mylist
codex mcp add mylist -- powershell.exe -NoLogo -NoProfile -NonInteractive -Command '$marker=Join-Path $env:LOCALAPPDATA "com.mylist.desktop\.install-location"; if (!(Test-Path -LiteralPath $marker -PathType Leaf)) { throw "MyLIST installation location is unavailable" }; $dir=(Get-Content -LiteralPath $marker -Raw).Trim(); $exe=Join-Path $dir "windows-client.exe"; if (!(Test-Path -LiteralPath $exe -PathType Leaf)) { throw "MyLIST executable is unavailable" }; & $exe --mcp-bridge; exit $LASTEXITCODE'
codex mcp get mylist
```

It is acceptable for `codex mcp remove mylist` to report that no such server exists. The `add` and `get` commands must succeed. Keep the launcher command exactly as shown and do not replace it with the current absolute MyLIST executable path.

## 4. Install a fresh Skill

Remove only the existing `mylist` Skill directory, then copy `<install-dir>\docs\Skill.md` to the local Codex Skills directory as the fresh `mylist` Skill. Use byte-preserving file copy operations. Do not change unrelated Skills.

The Skill uses the case-insensitive standalone trigger word `mylist`. With that explicit trigger, it can create a task with or without a due time and must use MyLIST rather than a Codex reminder.

## 5. Validate the clean installation

Validate only the local configuration state:

- `codex mcp get mylist` succeeds;
- the configured MyLIST command is `powershell.exe` and does not contain an installation-specific MyLIST path;
- the launcher reads `%LOCALAPPDATA%\com.mylist.desktop\.install-location` and passes `--mcp-bridge`;
- the location record resolves to an existing `windows-client.exe`;
- the installed `mylist` Skill matches the bundled `docs\Skill.md`;
- there is exactly one `[mcp_servers.mylist]` entry and no old absolute MyLIST executable path remains in the Codex configuration;
- the complete Codex configuration still loads successfully and all unrelated project and MCP entries remain unchanged;
- non-ASCII paths in the configuration match the byte-for-byte backup when decoded as UTF-8.

Do not launch or control the MyLIST interface, do not change its Local MCP service switch, and do not require a live MCP connection as part of installation. Restart Codex if it requires a restart to reload MCP or Skill configuration.

## 6. First use and connection check

Only actual MCP use requires MyLIST to be running and **Settings → AI connection → Local MCP service** to be enabled. MyLIST may stay in the tray.

After those runtime conditions are met, verify the connection in a new Codex chat with: `mylist Read my to-do items and tell me the count.` A successful response confirms that Codex started the bridge, completed the MCP handshake, and can call MyLIST tools. If MyLIST is not running or the local service is off, the bridge returns an offline error and does not start MyLIST.

## 7. Reinstalling MyLIST in a different location

No Codex MCP reconfiguration is required after this location-independent configuration has been installed. A successful MyLIST installation automatically updates `.install-location`, and the next MCP launch resolves the new executable. Reinstall the integration only when upgrading from an older configuration that directly referenced `windows-client.exe`, or when the bundled Skill itself needs to be refreshed.

## 8. Permissions, privacy, and removal

- The bridge connects only to the local `\\.\pipe\MyLIST-MCP` named pipe. It does not open a network port.
- Reads and normal edits are checked by both Codex tool permissions and MyLIST.
- Deletion, replace import, and export require visible confirmation in MyLIST.
- Export destinations, import files, and encryption passwords are selected or entered only in MyLIST. The AI agent never receives a password or a complete local path.
- To disconnect immediately, turn off Local MCP service in MyLIST. To remove the Codex integration, remove only the `mcp_servers.mylist` configuration and the installed `mylist` Skill. MyLIST data is not deleted.
- Uninstalling MyLIST removes `.install-location`. The `mylist.sqlite3` task database is preserved unless the user explicitly chooses to delete all task data.
