# lunar-serve

**A highly decoupled, warning-free, read-only HTTP distribution layer for LunarAST ecosystem data.**

`lunar-serve` is a lightweight, zero-dependency HTTP server built with Axum. It consumes `lunar-map.json` and serves both human-readable Markdown contracts, structured JSON, and **on-demand raw source code mirroring** directly to AI agents with zero manual configuration.

---

## 🏛️ Decoupled Architecture

`lunar-serve` is strictly decoupled from CLI-specific operations (I/O, S3 SDKs, cryptography, terminal prompts), relying purely on the lightweight `lunar-interface` core model:

```
lunar-serve/
├── Cargo.toml
└── src/
    ├── lib.rs            # Project registry deserialization & case-insensitive matching
    ├── main.rs           # Minimalist entrypoint and controller handlers
    ├── render.rs         # Markdown, Mermaid, collapsible directory tree, AI instruction loader
    ├── utils.rs          # Safe file IO, JSONL rolling writer, daily log purger
    ├── session.rs        # In-memory session manager with CSRF binding
    ├── lct.rs            # Ed25519-signed LunarAST Cryptographic Token
    ├── totp.rs           # TOTP validation (HMAC-SHA1, 6 digits, ±30s tolerance)
    ├── patch.rs          # AI patch parser (---LUNAR_PATCH_START--- format)
    └── handlers/         # Route handler modules (core, raw, todo, security)
```

---

## ⚡ Quick Deployment

Ensure you have generated the global topography map using the `lunar` CLI first.

### Installation

#### Option A: Download a pre-built binary (fastest)
Pre-built binaries are available for **Linux amd64** on the [GitHub Releases](https://github.com/LunarAST/lunar-serve/releases) page.

1. Download `lunar-serve` and the accompanying `checksums.txt` from the latest release.
2. (Recommended) Verify integrity:
   ```bash
   sha256sum -c checksums.txt
   ```
3. Make the binary executable and move it to a directory in your `PATH`:
   ```bash
   chmod +x lunar-serve
   sudo mv lunar-serve /usr/local/bin/
   ```

> **Note**: Currently only Linux amd64 is provided. For other platforms, compile from source (see below).

#### Option B: Compile from source
```bash
cargo build --release -p lunar-serve
# The binary will be located at target/release/lunar-serve
```

### Option 1: One-Click Launch via lunar CLI (Recommended)
In any terminal path, simply run:
```bash
lunar
```
And select `[5] Launch serving daemon` from the menu.  
The CLI also allows stopping (`8`) and restarting (`9`) the daemon via PID file.

### Option 2: Run directly from binary
```bash
lunar-serve /opt/lunar-map.json
```
*   **Default Port**: Listens on `http://0.0.0.0:8787`.
*   **Port Override**: Export `LUNAR_SERVE_PORT=8080` to override.
*   **Host Domain Mapping**: Export `LUNAR_SERVE_DOMAIN="https://your-domain.com"` to declare your primary public domain. The server dynamically falls back to HTTP Host header sniffing if this is omitted.

---

## 🤖 AI Native Endpoints & Multimodal Negotiation

`lunar-serve` is designed as a secure, stateless, read-only code interface for AI Agentic consumption.

### 1. Canvas Index (GET `/`)
Serves the single-file compiled React canvas of `lunar-scope` natively. Opening `http://127.0.0.1:8787/` in your browser instantly loads the topology chart, with zero CORS friction and zero Nginx static hosting configurations required.

### 2. File Tree & Contract Summary (Accept-based Multimodal Response)
*   **Endpoint**: `GET /:owner/:repo/tree/:branch`
*   **Behavior**: 
    *   **Default (Markdown)**: Returns a beautifully formatted RouteAST contract summary, a collapsible active todo list, and a **recursive file directory tree of your local VPS workspace** (excluding build noise like `target/`, `node_modules/`, and `.pyc` files). Prepend `#` comments inside the tree code block to guide AI navigation.
    *   **Negotiated (JSON)**: If requested with `Accept: application/json`, it bypasses markdown rendering and returns a high-cohesion JSON array containing all clean relative file paths for easy programmatic parsing.

### 3. On-Demand Source Code Reading (Zero-Copy Raw Streaming)
*   **Endpoint**: `GET /:owner/:repo/raw/:branch/*filepath` (or `/blob/` alias)
*   **Behavior**: Returns the raw text of the requested file securely. It automatically checks for directory traversal attacks via canonical path comparison and validates access permissions.

### 4. AI Handover Todo Scratchpad
*   **GET `/api/v1/projects/:name/todo`**: Retrieves the current AI task list and pending contract patches.
*   **POST `/api/v1/projects/:name/todo`**: Updates tasks and registers cryptographic handovers.
*   **GET `/api/v1/projects/:name/todo/diff`**: Returns a side-by-side comparative Markdown diff of the pending patch against your current contract for peer-review AI models.

---

## 🔐 v3.0 Security Layer & Global AI Instruction

### Human Login & Session
*   **POST `/login`** – Accepts a 6‑digit TOTP code. On success, issues an `HttpOnly; Secure; SameSite=Strict` session cookie and returns a CSRF token.
*   **GET `/csrf-token`** – Returns the CSRF token for the current session.

### LCT Token Generation (AI Read-Only Access)
*   **POST `/token/generate`** – Requires session + CSRF. Generates a time‑limited, resource‑bound LCT (Ed25519‑signed). The token is automatically corrected to use the real branch from `repos.json`.
*   **GET `/t/:token/:owner/:repo/tree/:branch`** – Validates the LCT token. If valid, serves the project page exactly like the public route, but without requiring a login session.

### Patch Dispatch (Human‑only, Two‑Factor)
*   **POST `/dispatch`** – Requires session, CSRF, and a valid TOTP code. Writes the submitted AI patch into `.lunar/suggestions/` for later CLI merging.

### Project Visibility Enforcement
Projects marked `"visibility": "private"` in `repos.json` will return `401 Unauthorized` unless accessed with a valid session cookie or LCT token. Public projects remain openly readable.

### Global AI Instruction
Place a `config/ai‑instruction.md` file inside the `lunar‑serve` directory. It is loaded at startup and rendered at the top of every project page. No project‑specific overrides – one source of truth for all repositories. A default template is included in the repository.

### Server PID File
On startup, `lunar‑serve` writes its process ID to `.lunar/lunar‑serve.pid`. This is used by the `lunar` CLI to stop/restart the server gracefully.

---

## 📜 License

Apache-2.0
