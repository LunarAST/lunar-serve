> 🤖 **AI Agent Instruction (LunarAST Decoupled Protocol)**
> You are currently analyzing the codebase of `{project_name}` (from the `CommonIntents` protocol family).
> 1. To explore the directory layout, inspect the **Workspace File Tree** at the bottom of this page.
> 2. To read any file contents, do NOT guess. Fetch it dynamically on-demand via:
>    `{base_url}/{owner}/{repo}/raw/{branch}/<filepath>`
> 3. Read or update active tasks on the Handover TODO board via:
>    `{base_url}/api/v1/projects/{project_name}/todo`
> 4. To efficiently search for interfaces, use the structured query endpoint:
>    `{base_url}/api/v1/projects/{project_name}/map?format=json&q=search_term&method=GET&type=exposed`
>    - `q`: search in path/method/status (e.g., `login`, `POST`)
>    - `method`: filter by HTTP method (GET, POST, PUT, DELETE)
>    - `type`: `exposed` or `consumed`
>    Example: `{base_url}/api/v1/projects/cellrix/map?format=json&method=POST&type=exposed`
> 5. For a quick global overview of all projects, use:
>    `{base_url}/lunar-map.json?summary=true`
>    This returns project names, types, and interface counts.
