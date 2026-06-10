# lunar-serve
**LunarAST 生态数据只读 HTTP 分发服务**

`lunar-serve` 是一款轻量、无运行时依赖的 HTTP 服务，专门用于读取由 `lunar map` 生成的全局拓扑文件 `lunar-map.json`。服务既支持展示易读的 Markdown 汇总内容，也可输出标准结构化 JSON 数据，同时提供参数化筛选能力，适配 AI 代理与可视化面板调用。

---

## 快速上手

### 1. 生成拓扑数据文件
```bash
lunar map -o /opt/lunar-map.json
```

### 2. 启动服务
```bash
lunar-serve /opt/lunar-map.json
```

服务默认监听地址：`http://0.0.0.0:8787`。
如需修改监听端口，执行以下命令配置环境变量：`export LUNAR_SERVE_PORT=8080`

### 3. 配置首个 GitHub 项目镜像
在 `lunar-map.json` 同级目录中创建 `repos.json` 文件：
```json
{
  "version": "0.5.0",
  "projects": [
    {
      "name": "my-service",
      "displayName": "My Service",
      "source": {
        "type": "github",
        "github": {
          "owner": "your-github-org",
          "repo": "my-service",
          "branch": "main"
        }
      },
      "visibility": "public"
    }
  ]
}
```

重启 `lunar-serve` 后，即可通过如下地址访问对应项目：
```
https://your-domain.com/your-github-org/my-service/tree/main
```

> **基础镜像功能无需执行 `lunar scan`**：镜像会直接展示已声明的接口信息。若需要自动采集接口数据，可在本地执行 `lunar scan`（需适配对应技术框架），或使用 AI 生成配置补丁。

---

## 接口列表

### 生态汇总接口
| 接口地址 | 说明 |
|:---|:---|
| `GET /lunar-map.md?summary=true` | 生态整体汇总内容（约 200 字符，适合 AI 初次接入读取） |
| `GET /lunar-map.md` | 完整拓扑结构（Markdown 格式） |
| `GET /lunar-map.md?scope=<project>` | 仅展示单个项目视图 |
| `GET /lunar-map.md?status=<orphaned\|unused>` | 按契约状态筛选（孤立接口 / 未使用接口） |
| `GET /lunar-map.md?path=<path>` | 根据接口路径关键字筛选 |
| `GET /lunar-map.md?style=mermaid` | 输出 Mermaid 格式拓扑图表 |
| `GET /lunar-map.json` | 完整拓扑结构（JSON 格式） |

### GitHub 镜像路由
在 `repos.json` 中配置为 GitHub 数据源的项目，可沿用 GitHub 原生路径风格进行访问：
```
GET /{owner}/{repo}/tree/{branch}
```

示例：`https://lunar.aifify.com/your-org/your-repo/tree/main`

该路由设计无论是人工使用，还是 AI 代理调用，都能快速识别访问目标。

### 统一标准接口
所有项目也可通过以下通用接口访问：
```
GET /api/v1/projects/{name}/map.md
GET /api/v1/projects/{name}/map.json
```

### 旧版本兼容接口
```
GET /project/{name}
```

### 健康检查接口
```
GET /healthz → 200 OK
```

---

## 为镜像添加项目

1. **编辑 `repos.json`**（与 `lunar-map.json` 放置在同一目录），在 `projects` 数组内新增项目配置。
   - 对接 GitHub 仓库请填写 `"type": "github"`
   - 本地项目请填写 `"type": "local"`

2. **重启 `lunar-serve`**，加载最新配置。

3. 镜像路由会立即生效。如果该项目已通过 `lunar scan` 或 AI 补丁完成接口声明，相关内容会同步展示在 Markdown 快照中。

> **使用提示**：可搭配 `lunar patch` 命令，快速导入 AI 生成的 YAML 补丁，自动补全接口信息。

---

## 配置说明
`lunar-serve` 会读取 `lunar-map.json` 同级目录下的 `repos.json` 文件，用于构建 GitHub 镜像映射关系，并完成项目身份校验。

### repos.json 配置格式
```json
{
  "version": "0.5.0",
  "projects": [
    {
      "name": "cellrix",
      "displayName": "Cellrix",
      "source": {
        "type": "github",
        "github": {
          "owner": "your-github-org",
          "repo": "cellrix",
          "branch": "main"
        }
      },
      "visibility": "public"
    },
    {
      "name": "my-internal-service",
      "displayName": "Internal Service",
      "source": { "type": "local" },
      "visibility": "private"
    },
    "my-library"
  ]
}
```

项目配置支持两种写法：
- **简易字符串**：默认判定为本地项目、公开访问权限
- **完整配置对象**：可自定义数据源（GitHub / 本地）、展示名称与访问权限

> **隐私说明**：仅标记为 `"visibility": "public"` 的项目可匿名访问。私有项目需通过 JWT 令牌鉴权（功能开发中）。

---

## 公网部署（零信任安全规范）
当你在本地运行 `lunar-serve` 并将服务对外暴露至公网、供 AI 代理调用时，**必须遵守以下安全基线要求**：

### 安全规则
1. **默认隐藏私有项目**：在 `repos.json` 中标记为 `"visibility": "private"` 的项目，不会在公开接口中对外暴露。
2. **敏感数据请使用私有接口**：涉及敏感数据时，请调用 `/private/` 前缀接口，此类接口强制开启 Ed25519-JWT 身份校验。
3. **私有数据禁止无鉴权暴露**：若使用隧道工具转发服务，请确保仅开放公开接口，或全局启用 JWT 鉴权，严禁私有数据脱离令牌校验直接对外暴露。

### 方案一：Cloudflare 隧道（免费、无需复杂配置）
```bash
# 安装 cloudflared 客户端
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 -o cloudflared
chmod +x cloudflared

# 启动 lunar-serve 服务
lunar-serve /opt/lunar-map.json

# 创建临时隧道
./cloudflared tunnel --url http://localhost:8787
```

执行后终端会生成公网访问地址（格式类似 `https://lunar-serve-xxxx.trycloudflare.com`），将该地址提供给 AI 代理即可。

### 方案二：Tailscale 端口转发（持久稳定、安全性高）
```bash
# 启动 lunar-serve 服务
lunar-serve /opt/lunar-map.json

# 通过 Tailscale 对外暴露端口（需提前安装 Tailscale）
tailscale funnel 8787
```

### 方案三：Ngrok 端口映射
```bash
# 启动 lunar-serve 服务
lunar-serve /opt/lunar-map.json

# 通过 Ngrok 对外暴露端口
ngrok http 8787
```

> **重要提醒**：以上所有隧道工具仅会对外转发**数据分发层**内容。你的源代码、本地原始文件**绝对不会**通过隧道被外部访问，服务仅对外提供 `lunar-map.json` 中的拓扑数据。

---

## 与其他组件的关联关系
| 组件 | 关联说明 |
|:---|:---|
| `lunar` 命令行工具 | 生成 `lunar-map.json` 文件，为本服务提供核心数据源 |
| `lunar-scope` | 在线运行模式下，可从本服务拉取 `lunar-map.json` 数据并完成可视化渲染 |
| RouteAST 子协议 | 定义汇总内容中展示的各类契约状态枚举 |

`lunar-serve` 是纯只读的数据分发服务，不会向磁盘写入任何文件，也不会修改现有配置。

---

## 许可证
Apache-2.0
