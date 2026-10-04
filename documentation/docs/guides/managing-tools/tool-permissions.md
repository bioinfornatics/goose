---
title: Managing Tool Permissions
sidebar_position: 1
sidebar_label: Tool Permissions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft, Tornado, Settings } from 'lucide-react';

Tool permissions provide fine-grained control over how goose uses different tools within extensions. This guide will help you understand and configure these permissions effectively.

## Understanding Tools and Extensions

Before diving into permissions, let's clarify the key components:

- **Extensions** are packages that add functionality to goose (like Developer, Google Drive, etc.)
- **Tools** are specific functions within each extension that goose can use

For example, the Developer extension includes multiple tools like:

- Text editor tool for file editing
- Shell tool for running commands
- Screen capture tool for taking screenshots
:::warning Performance Optimization
goose performs best with fewer than 25 total tools enabled across all extensions. Consider enabling only the extensions you need for your current task.
:::

## Permission Levels

Tool permissions work alongside [goose permission modes](/docs/guides/managing-tools/goose-permissions). The mode sets the default behavior, while tool permissions let you override the behavior of specific tools.

Each tool can be set to one of three permission levels:

| Permission Level | Description | Best For | Examples |
|-----------------|-------------|-----------|----------|
| **Always Allow** | Tool runs without requiring approval | Safe, read-only operations | • File reading<br></br>• Directory listing<br></br>• Information retrieval |
| **Ask Before** | Requires confirmation | State-changing operations | • File writing/editing<br></br>• System commands<br></br>• Resource creation |
| **Never Allow** | Tool cannot be used | Sensitive operations | • Credential access<br></br>• System-critical files<br></br>• Resource deletion |

## Configuring Tool Permissions

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    You can configure fine-grained tool permissions for enabled extensions when using `Manual` or `Smart` approval mode. These rules can be accessed from the mode toggle or `Settings` page.

    <Tabs>
      <TabItem value="toggle" label="Mode Toggle" default>
        1. Click the <Tornado className="inline" size={16} /> button at the bottom of the app
        2. Click the <Settings className="inline" size={16} /> button next to your selected `Manual` or `Smart` mode
        3. Click the extension whose tools you want to configure
        4. Use the dropdown next to each tool to set its permission level
        5. Click `Save Changes`
      </TabItem>
      <TabItem value="settings" label="Settings Page" default>
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
        2. Click the `Settings` button on the sidebar
        3. Click `Chat`
        4. Under `Mode`, click the <Settings className="inline" size={16} /> button next to your selected `Manual` or `Smart` mode
        5. Click the extension whose tools you want to configure
        6. Use the dropdown next to each tool to set its permission level
        7. Click `Save Changes`
      </TabItem>
    </Tabs>
  
  </TabItem>
  <TabItem value="cli" label="goose CLI">

    1. Run the configure command:
    ```sh
    goose configure
    ```

    2. Select `goose settings` from the menu
    ```sh
    ┌ goose-configure
    │
    ◆ What would you like to configure?
    | ○ Configure Providers
    | ○ Add Extension
    | ○ Toggle Extensions
    | ○ Remove Extension
    // highlight-start
    | ● goose settings
    // highlight-end
    └
    ```

    3. Choose `Tool Permission`
    ```sh
    ┌   goose-configure
    │
    ◇  What would you like to configure?
    │  goose settings
    │
    ◆  What setting would you like to configure?
    │  ○ goose mode
    // highlight-start
    │  ● Tool Permission
    // highlight-end
    |  ○ Tool Output
    └
    ```

    4. Select an extension and configure permissions for its tools:
    ```sh
    ┌   goose-configure
    │
    ◇  What setting would you like to configure?
    │  Tool Permission 
    │
    ◇  Choose an extension to configure tools
    │  developer 
    │
    ◇  Choose a tool to update permission
    │  read_image 
    │
    ◆  Set permission level for tool read_image, current permission level: Not Set
    │  ○ Always Allow 
     // highlight-start
    │  ● Ask Before (Prompt before executing this tool)
    // highlight-end
    │  ○ Never Allow 
    └
    ```
  </TabItem>
</Tabs>

## Scoped Permissions

The `/permissions` host command manages the tools attached to the active session without sending a message to the model. Rules can be saved at one of these writable scopes:

| Scope | Lifetime | Storage behavior |
| --- | --- | --- |
| **This session** | Current session | Kept in memory and removed with the session |
| **This project, for me** | Current checkout and user | Stored in `.config/goose/permission.local.yaml` under the session working directory |
| **This project, shared** | Current project | Stored in `.config/goose/permission.yaml`; until workspace trust is implemented, shared rules are restrictive-only and cannot use **Always Allow** |
| **All sessions on this device** | Current user | Stored in the existing goose `permission.yaml` file |

The Desktop distinguishes the tools being shown from the destination being changed:

- **Showing** identifies the active session or extension filter.
- **Saving to** identifies the persistence scope for the rule.

The CLI supports the same model interactively and with explicit subcommands:

```text
/permissions list
/permissions set developer__shell ask --scope project-local
/permissions reset developer__shell --scope session
```

Valid CLI scope values are `session`, `project-local`, `project-shared`, and `user`. Valid effects are `allow`, `ask`, and `deny`.

### Resolution and provenance

All applicable rules participate in one deterministic resolution:

1. `deny` wins over `ask`, and `ask` wins over `allow` across scopes.
2. For rules with the same effect, scope precedence is `managed` > `session` > `project-local` > `project-shared` > `user`.
3. For the same effect and scope, a function rule is more specific than an extension rule, which is more specific than a capability rule.

The UI reports the effective permission, the scope and origin of the winning rule, and the other applicable rules. A local `allow` cannot silently override a broader `deny`.

:::warning Permissions are not a sandbox
Permission rules decide whether goose may call a tool. They do not constrain what an allowed process can do at the operating-system level. Command aliases, interpreters, scripts, or indirect execution require separate sandboxing controls.
:::

For the complete lifetime, inheritance, trust, and fail-closed contract, see [Scoped permissions design](./scoped-permissions-design).

## Deterministic Shell Safety Policy

The built-in Developer shell applies a deterministic safety policy before permission-mode and Smart Approval decisions:

| Command family | Decision |
| --- | --- |
| `sudo`, `doas`, or `su` | Deny |
| Recursive forced `rm` targeting filesystem root, including `rm -rf /`, `rm -fr /`, split flags, long flags, and `/bin/rm` | Deny |
| Other `rm` commands | Ask |
| Safely parsed commands that match no policy | Continue to normal permission evaluation |
| Dynamic or unsupported shell syntax | Ask |

The evaluator tokenizes a bounded subset of POSIX shell syntax, separates linear chains such as `;`, `&&`, `||`, pipes, and newlines, unwraps selected transparent wrappers, and applies the most restrictive decision across all subcommands. Policy tests use command strings only and never execute destructive commands.

:::warning Command policy is not a sandbox
Interpreters, mutable scripts, aliases, indirect filesystem APIs, and syntax outside the bounded parser can have equivalent effects without matching a command rule. Unknown syntax asks for approval, but an OS sandbox and a non-privileged account remain necessary security boundaries.
:::

## Benefits of Permission Management

:::tip
Review and update your tool permissions as your tasks change. You can modify permissions at any time during a session.
:::

There are several reasons to configure tool permissions:

1. **Performance Optimization**
   - Keep total enabled tools under 25 for best performance
   - Disable tools you don't need for your current task
   - Reduce context window usage and improve response quality
   - Prevent tool decision paralysis

2. **Security Control**
   - Restrict access to sensitive operations
   - Prevent accidental file modifications
   - Control system resource usage

3. **Task Focus**
   - Enable only tools needed for current task
   - Help goose make better tool choices
   - Reduce noise in responses

## Example Permission Configuration

### Task-Based Configuration

Configure permissions based on your current task:

```
Development Task:
✓ File reading → Always Allow
✓ Code editing → Ask Before
✓ Test running → Always Allow
✗ System commands → Ask Before

Documentation Task:
✓ File reading → Always Allow
✓ Markdown editing → Always Allow
✗ Code editing → Never Allow
✗ System commands → Never Allow
```