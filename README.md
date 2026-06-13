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
    ├── main.rs           # Minimalist entrypoint and controller handlers (<150 lines)
    ├── render.rs         # Markdown, Mermaid, and collapsible directory tree renderer
    └── utils.rs          # Safe file IO, JSONL rolling writer, and daily log purger
```

---

## ⚡ Quick Deployment

Ensure you have generated the global topography map using the `lunar` CLI first.

### Option 1: One-Click Launch via lunar CLI (Recommended)
In any terminal path, simply run:
```bash
lunar
```
And select `[5] Launch serving daemon` from the menu.

### Option 2: Run directly from binary
```bash
lunar-serve /opt/lunar-map.json
```
*   **Default Port**: Listens on `http://0.0.0.0:8787`.
*   **Port Override**: Export `LUNAR_SERVE_PORT=8080` to override.
*   **Host Domain Mapping**: Export `LUNAR_SERVE_DOMAIN="https://lunar.aifify.com"` to declare your primary public domain. The server will dynamically fall back to HTTP Host header sniffing if this is omitted.

---

## 🤖 AI Native Endpoints & Multimodal Negotiation

`lunar-serve` is designed as a secure, stateless, read-only code interface for AI Agentic consumption.

### 1. Canvas Index (GET `/`)
Serves the single-file compiled React canvas of `lunar-scope` natively. Opening `http://127.0.0.1:8787/` in your browser instantly loads the 3D topology chart, with zero CORS friction and zero Nginx static hosting configurations required.

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

## 📜 License

Apache-2.0
