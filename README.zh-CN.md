# lunar-serve
**LunarAST 生态数据只读 HTTP 分发层**

`lunar-serve` 是一套轻量、无运行时依赖的 HTTP 服务，用于对外读取由 `lunar map` 生成的全局拓扑文件 `lunar-map.json`。它同时提供易读的 Markdown 汇总内容与标准结构化 JSON 数据，并且支持参数化筛选功能，可对接 AI 代理与可视化面板。

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
如需修改端口，执行环境变量配置：`export LUNAR_SERVE_PORT=8080`

### 3. 配置首个 GitHub 项目镜像
在 `lunar-map.json` 同级目录下创建 `repos.json` 文件：
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

重启 `lunar-serve` 后，即可通过以下地址访问对应项目：
```
https://你的域名/your-github-org/my-service/tree/main
```

> **基础镜像功能无需执行 `lunar scan`** —— 镜像会直接展示已声明的接口信息。如果需要自动抓取接口数据，可在本地执行 `lunar scan`（适配对应技术栈框架），或使用 AI 生成配置补丁。

---

## 接口列表

### 生态汇总接口
| 接口地址 | 说明 |
|:---|:---|
| `GET /lunar-map.md?summary=true` | 生态整体汇总（约 200 字符，适合 AI 初次读取） |
| `GET /lunar-map.md` | 完整拓扑结构（Markdown 格式） |
| `GET /lunar-map.md?scope=<project>` | 仅展示单个项目视图 |
| `GET /lunar-map.md?status=<orphaned\|unused>` | 按契约状态筛选（孤立接口 / 未使用接口） |
| `GET /lunar-map.md?path=<path>` | 按接口路径关键字筛选 |
| `GET /lunar-map.md?style=mermaid` | 输出 Mermaid 拓扑图表 |
| `GET /lunar-map.json` | 完整拓扑结构（JSON 格式） |

### GitHub 镜像路由
在 `repos.json` 中配置为 GitHub 来源的项目，可沿用 GitHub 原生路径风格访问：
```
GET /{owner}/{repo}/tree/{branch}
```

示例：`https://lunar.aifify.com/your-org/your-repo/tree/main`

该设计无论是人工使用还是 AI 调用，都能快速识别路由含义。

### 统一标准接口
所有项目也可通过以下标准接口访问：
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

## 为镜像新增项目

1. **编辑 `repos.json`**（与 `lunar-map.json` 放在同一目录），在 `projects` 数组中新增项目配置。
   - 如需对接 GitHub 仓库，填写 `"type": "github"`
   - 本地项目填写 `"type": "local"`

2. **重启 `lunar-serve`**，加载最新配置。

3. 镜像路由即刻生效。如果该项目已通过 `lunar scan` 或 AI 补丁声明接口，相关内容会同步展示在 Markdown 快照中。

> **小提示**：可结合 `lunar patch` 命令，快速将 AI 生成的 YAML 补丁导入并补全接口信息。

---

## 配置说明
`lunar-serve` 会读取 `lunar-map.json` 同级目录下的 `repos.json`，用于构建 GitHub 镜像映射关系，并校验项目身份信息。

### repos.json 完整格式
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
- **简易字符串**：默认视为本地项目、公开访问权限
- **完整对象**：可配置数据源（GitHub / 本地）、展示名称、访问权限

> **权限说明**：仅标记为 `"visibility": "public"` 的项目可匿名访问。私有项目需通过 JWT 令牌鉴权（功能开发中）。

---

## 与其他组件的关联关系
| 组件 | 关联说明 |
|:---|:---|
| `lunar` 命令行工具 | 生成 `lunar-map.json` 文件，为本服务提供数据源 |
| `lunar-scope` | 在线环境下，可从本服务拉取 `lunar-map.json` 数据完成可视化渲染 |
| RouteAST 子协议 | 定义汇总内容中展示的各类契约状态 |

`lunar-serve` 仅作为**只读数据分发服务**，不会向磁盘写入任何文件，也不会修改现有配置。

---

## 许可证
Apache-2.0
