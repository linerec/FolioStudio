The first public build of **Folio Studio**: open-source desktop publishing in which the project folder is the document.

### What's inside
- **Project folders that travel.** Links, pictures and fonts are stored with project-relative paths and SHA-256 hashes, so a folder opens as it is on another drive, a NAS, Windows or macOS.
- **INDD and IDML import, IDML export.** Anything Folio doesn't understand yet is kept verbatim and written back out. Import and export reports list what will change.
- **Print output.** PDF/X-4, X-1a and X-3 with transparency flattening, preflight, separations preview, soft proofing with your installed ICC profiles, and packaging.
- **Typography.** Paragraph composer, hyphenation, optical kerning, Korean typesetting, footnotes, tables, text on a path, MathML, indexes, books and data merge.
- **MCP preview, included in both downloads.** Enable **Preferences ▸ General ▸ Allow LLM connection (MCP)** and start the bundled `folio-mcp` with `--attach`. The connected Windows build lists 178 tools for documents, selections, pages, dialogs, panels and selected edits, including two tools that report connection status and coverage. Saving and exporting require file-write permission. Refreshing a database source requires its own permission. All three switches start off. Passwords and printing stay outside MCP. [Connection instructions and Claude Desktop example](https://github.com/linerec/FolioStudio#-connect-an-ai-mcp)

### Downloads

These archives replace the earlier downloads under the same v0.1.0-alpha tag.

| File | For | Bytes | SHA-256 |
|---|---|---:|---|
| `FolioStudio-0.1.0-alpha-windows-x64.zip` | Windows 10/11, x64 | 202,139,158 | `0e8666f7c0b61dd96b4dfed7e48b584917d5005f7a3f5c5595f32e40f348b886` |
| `FolioStudio-0.1.0-alpha-macos-arm64.zip` | macOS, Apple Silicon (experimental) | 171,399,782 | `f070c75c7f4c29f66652650d2a6540a03a1d62f338b44d45ae5a9673c77832af` |

Unzip and run **Folio Studio**. To see what it can do, open the [`samples/low-water`](https://github.com/linerec/FolioStudio/tree/main/samples/low-water) folder with **File ▸ Open Project**.

### Known limitations
- This is an **alpha**. Expect rough edges and format changes before 1.0.
- Windows has no publisher signature. The macOS app has an ad-hoc signature and has not been notarized. SmartScreen or Gatekeeper may prompt on first launch.
- The macOS build is cross-compiled. MCP attachment was tested with the Windows download. Attachment on macOS and Linux still needs testing on those platforms.

---

**한국어** · v0.1.0-alpha 배포 파일을 갱신했습니다. 두 ZIP 모두 MCP 미리보기 서버가 들어 있습니다. 압축을 풀고 Folio Studio를 실행한 뒤, AI를 연결하려면 환경 설정 ▸ 일반에서 연결을 켜세요. Windows 배포본으로 연결과 편집을 확인했습니다. macOS와 Linux 연결은 실기 확인 전입니다. [AI 연결 방법](https://github.com/linerec/FolioStudio/blob/main/README.ko.md#-ai-연결mcp)
