# lunar-serve

**高度解耦、零警告、只读的 LunarAST 生态数据 HTTP 分发层。**

`lunar-serve` 是一个基于 Axum 构建的轻量级、零运行时依赖的 HTTP 服务器。它消费 `lunar-map.json`，并为 AI 代理提供人类可读的 Markdown 契约、结构化 JSON 以及**按需的原始源代码镜像**，无需任何人工配置。

---

## 🏛️ 解耦架构

`lunar-serve` 与 CLI 特有的操作（I/O、S3 SDK、密码学、终端提示）严格解耦，仅依赖轻量的 `lunar-interface` 核心模型：

```
lunar-serve/
├── Cargo.toml
└── src/
    ├── lib.rs            # 项目注册表反序列化 & 大小写不敏感匹配
    ├── main.rs           # 极简入口和控制器处理程序（<150 行）
    ├── render.rs         # Markdown、Mermaid 和可折叠目录树渲染器
    └── utils.rs          # 安全的文件 IO、JSONL 滚动写入器、日志每日清理
```

---

## ⚡ 快速部署

请确保已先用 `lunar` CLI 生成全局拓扑地图。

### 方式一：通过 `lunar` CLI 一键启动（推荐）
在任意终端路径下，直接运行：
```bash
lunar
```
然后从菜单中选择 `[5] Launch serving daemon`。

### 方式二：直接从二进制运行
```bash
lunar-serve /opt/lunar-map.json
```
*   **默认端口**：监听 `http://0.0.0.0:8787`。
*   **端口覆盖**：设置环境变量 `LUNAR_SERVE_PORT=8080` 覆盖。
*   **主机域名映射**：设置 `LUNAR_SERVE_DOMAIN="https://lunar.aifify.com"` 声明你的公网主域名。如果未设置，服务器将动态回退到 HTTP Host 头嗅探。

---

## 🤖 AI 原生端点与多模态协商

`lunar-serve` 被设计为面向 AI Agent 消费的安全、无状态、只读代码接口。

### 1. 画布索引（GET `/`）
原生提供单文件编译后的 `lunar-scope` React 画布。在浏览器中打开 `http://127.0.0.1:8787/`，即可直接加载 3D 拓扑图，无需任何 CORS 摩擦，也无需配置 Nginx 静态托管。

### 2. 文件树与契约摘要（基于 Accept 的多模态响应）
*   **端点**：`GET /:owner/:repo/tree/:branch`
*   **行为**：
    *   **默认（Markdown）**：返回格式优美的 RouteAST 契约摘要、可折叠的活跃待办列表，以及你的本地 VPS 工作区的**递归文件目录树**（已自动排除 `target/`、`node_modules/`、`.pyc` 等构建噪音）。你可以在树代码块内通过 `#` 开头的注释来引导 AI 导航。
    *   **协商（JSON）**：如果请求携带 `Accept: application/json`，则跳过 Markdown 渲染，返回一个高内聚的 JSON 数组，包含所有干净的相对文件路径，便于程序化解析。

### 3. 按需源代码读取（零拷贝原始流）
*   **端点**：`GET /:owner/:repo/raw/:branch/*filepath`（或 `/blob/` 别名）
*   **行为**：安全地返回请求文件的原始文本。它会通过规范路径比对自动检测目录遍历攻击，并验证访问权限。

### 4. AI 交接待办看板
*   **GET `/api/v1/projects/:name/todo`**：获取当前的 AI 任务列表和待合并的契约补丁。
*   **POST `/api/v1/projects/:name/todo`**：更新任务并注册密码学交接数据。
*   **GET `/api/v1/projects/:name/todo/diff`**：返回当前契约与待合并补丁的并排 Markdown 差异对比，供同行评审 AI 模型使用。

---

## 📜 许可证

Apache-2.0
