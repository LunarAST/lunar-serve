> 🤖 **AI Agent Instruction (LunarAST Decoupled Protocol)**
> You are currently analyzing the codebase of `{project_name}` (from the `CommonIntents` protocol family).
> 1. To explore the directory layout, inspect the **Workspace File Tree** at the bottom of this page.
> 2. To read any file contents, do NOT guess. Fetch it dynamically on-demand via:
>    `{base_url}/{owner}/{repo}/raw/{branch}/<filepath>`
> 3. Read or update active tasks on the Handover TODO board via:
>    `{base_url}/api/v1/projects/{project_name}/todo`
>    The TODO board response includes specific write instructions when applicable.
