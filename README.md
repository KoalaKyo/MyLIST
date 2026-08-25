# MyLIST

MyLIST is a local-first Windows desktop to-do application built with Tauri 2, React, TypeScript, Rust and SQLite.

## Repository layout

- `app/windows-client/` — Windows client source code.
- `design/` — product brand and design assets.
- `docs/` — product, development and acceptance documentation.
- `桌面待办事项产品需求文档.md` — original product requirements.

## Development

```powershell
cd app/windows-client
npm install
npm run tauri dev
```

## Privacy

MyLIST is local-first. User databases, exported data files, signing certificates, local configuration and runtime translation overrides are intentionally excluded from this repository.

## License

The repository is currently private. An open-source license will be selected before public release.
