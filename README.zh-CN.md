# lunar-serve
**LunarAST 生态高解耦、无警告、只读式 HTTP 数据分发层**

`lunar-serve` 是基于 Axum 构建的轻量零依赖 HTTP 服务。读取全局拓扑文件 `lunar-map.json`，无需人工配置，即可向AI智能体提供可读性 Markdown 接口契约、结构化 JSON 数据，以及**按需源码镜像直读**能力。

---

## 🏛️ 解耦式架构
`lunar-serve` 与命令行专属能力（文件IO、对象存储SDK、加密运算、终端交互）完全隔离，仅依赖轻量化 `lunar-interface` 核心模型：
```
lunar-serve/
├── Cargo.toml
└── src/
    ├── lib.rs            # 项目注册表反序列化、大小写兼容匹配逻辑
    ├── main.rs           # 极简程序入口与路由控制器
    ├── render.rs         # Markdown、Mermaid、可折叠目录树、AI全局指令加载器
    ├── utils.rs          # 安全文件读写、JSONL 滚动日志、日志自动清理工具
    ├── session.rs        # 内存会话管理器，绑定CSRF防护令牌
    ├── lct.rs            # Ed25519 签名 LunarAST 加密访问令牌
    ├── totp.rs           # TOTP 校验模块（HMAC-SHA1、6位验证码、±30秒时间容错）
    ├── patch.rs          # AI 变更补丁解析器（---LUNAR_PATCH_START--- 标准格式）
    └── handlers/         # 路由处理器模块（核心接口、源码读取、任务面板、安全校验）
```

---

## ⚡ 快速部署
部署前需先用 `lunar` 命令行工具生成全局拓扑映射文件。

### 安装方式
#### 方案A：下载预编译二进制包（推荐，速度最快）
GitHub Releases 发布页提供 **Linux amd64** 架构预编译程序：
1. 从最新版本下载 `lunar-serve` 主程序与校验文件 `checksums.txt`
2. （推荐）校验程序文件完整性：
```bash
sha256sum -c checksums.txt
```
3. 赋予执行权限并移动至系统环境变量目录：
```bash
chmod +x lunar-serve
sudo mv lunar-serve /usr/local/bin/
```

> 备注：当前仅提供 Linux amd64 二进制包，macOS、Windows、ARM 架构请使用下方源码编译方案。

#### 方案B：本地源码编译
```bash
cargo build --release -p lunar-serve
# 编译产物存放路径：target/release/lunar-serve
```

### 启动方式一：通过 lunar 命令一键启动（推荐）
任意终端目录直接执行：
```bash
lunar
```
在交互菜单选择 `[5] 启动分发服务`。
命令行同时支持通过 PID 文件执行停止（选项8）、重启（选项9）操作。

### 启动方式二：直接运行二进制程序
```bash
lunar-serve /opt/lunar-map.json
```
*   **默认监听端口**：`http://0.0.0.0:8787`
*   **自定义端口**：配置环境变量 `LUNAR_SERVE_PORT=8080` 覆盖默认端口
*   **公网域名配置**：配置环境变量 `LUNAR_SERVE_DOMAIN="shturl.cc/BGtfDfzzM8H9V"` 指定对外域名；未配置时服务会自动通过 HTTP Host 请求头自适应域名

---

## 🤖 AI 原生接口与多格式自适应协商
`lunar-serve` 专为AI智能体设计无状态、安全只读代码访问接口。

### 1. 可视化总览页面（GET `/`）
原生内置编译后的单文件 `lunar-scope` 前端画布。浏览器访问 `http://127.0.0.1:8787/` 即可加载拓扑可视化图表，无需配置Nginx静态资源托管，无跨域限制。

### 2. 文件树与接口契约概览（基于Accept请求头自适应多格式返回）
*   **接口地址**：`GET /:owner/:repo/tree/:branch`
*   **返回逻辑**
    *   **默认格式（Markdown）**：返回排版美观的 RouteAST 接口契约汇总、可折叠待办任务列表，以及服务器本地工作区递归文件目录树（自动过滤 `target/`、`node_modules/`、`.pyc` 等编译缓存文件）。目录树代码块内支持 # 注释，用于引导AI定位文件。
    *   **协商格式（JSON）**：请求携带 `Accept: application/json` 时，跳过Markdown渲染，返回结构规整的JSON数组，包含所有干净相对文件路径，便于程序解析。

### 3. 按需源码读取（零拷贝流式返回）
*   **接口地址**：`GET /:owner/:repo/raw/:branch/*filepath`（别名 `/blob/`）
*   **返回逻辑**：安全返回目标文件原始文本。通过标准化路径比对防御目录穿越攻击，并校验访问权限。

### 4. AI 交接任务面板
*   **GET `/api/v1/projects/:name/todo`**：获取当前AI任务列表与待合并接口补丁
*   **POST `/api/v1/projects/:name/todo`**：更新任务，登记加密交接记录
*   **GET `/api/v1/projects/:name/todo/diff`**：返回待合并补丁与当前契约的对比Markdown，供AI交叉评审

---

## 🔐 v3.0 安全层与全局AI指令文档
### 人工登录与会话管理
*   **POST `/login`**：接收6位TOTP验证码，校验通过后下发 HttpOnly; Secure; SameSite=Strict 会话Cookie，并返回CSRF令牌
*   **GET `/csrf-token`**：获取当前会话绑定的CSRF防护令牌

### LCT 只读访问令牌生成（AI专用）
*   **POST `/token/generate`**：需携带有效会话与CSRF令牌，生成带时效、绑定项目资源的Ed25519签名LCT令牌；令牌会自动匹配 `repos.json` 内真实分支信息
*   **GET `/t/:token/:owner/:repo/tree/:branch`**：校验LCT令牌有效性，校验通过后无需登录即可访问项目页面

### 补丁提交面板（仅人类可操作，双重校验）
*   **POST `/dispatch`**：必须携带会话、CSRF令牌与有效TOTP验证码，将AI生成的补丁写入 `.lunar/suggestions/`，后续由命令行执行合并

### 项目可见性权限控制
`repos.json` 中标记 `"visibility": "private"` 的私有项目，无有效会话Cookie或LCT令牌访问时，直接返回 401 未授权；公开项目可任意读取。

### 全局AI指令文档
在 `lunar-serve` 根目录放置 `config/ai-instruction.md`，服务启动时加载，所有项目页面顶部统一展示。不支持单项目独立覆盖，全仓库共用一套规则，仓库内置默认模板。

### 服务进程PID文件
服务启动后会将进程ID写入 `.lunar/lunar-serve.pid`，供 `lunar` 命令行优雅停止、重启服务。

---

## 📜 开源协议
Apache-2.0
