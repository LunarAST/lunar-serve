> 🤖 **LunarAST AI-Navigational Map (Decoupled MCI)**
> *You are currently analyzing the codebase of `{project_name}` (from the `CommonIntents` protocol family).*
> ├── 📂 Workspace Layout: Scan the "Workspace File Tree" at the bottom of this page.
> ├── 📄 File Fetcher: `{base_url}/{owner}/{repo}/raw/{branch}/<filepath>` (Fetch raw content directly; do NOT guess)
> ├── 📋 Handover Board: `{base_url}/api/v1/projects/{project_name}/todo` (GET to inspect, POST to update active tasks)
> ├── 🔍 Interface Query: `{base_url}/api/v1/projects/{project_name}/map?format=json&q=<term>&method=<GET|POST>&type=<exposed|consumed>`
> │   ├── *Filter keys*: `q` (search in path/method/status), `method` (HTTP verb), `type` (`exposed` | `consumed`)
> │   └── *Example*: `{base_url}/api/v1/projects/cellrix/map?format=json&method=POST&type=exposed`
> └── 🌐 Ecosystem Overview: `{base_url}/lunar-map.json?summary=true` (Returns project names, types, and interface counts)
