---
title: Azure AI Foundry
description: Connect goose to Azure AI Foundry serverless model endpoints (MaaS)
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Azure AI Foundry

[Azure AI Foundry](https://ai.azure.com) serverless (Model-as-a-Service) endpoints host third-party and Microsoft models — Phi-4, Mistral, Llama, Cohere, and others — as fully managed APIs. goose connects to these endpoints using the `azure_foundry` provider.

## Prerequisites

- An Azure subscription with access to Azure AI Foundry
- A deployed serverless model endpoint in your Foundry project (`.models.ai.azure.com`)
- Either an API key for that endpoint, or Azure CLI (`az login`) / managed identity for Entra ID auth

## Step 1: Find Your Endpoint

Navigate to your [Azure AI Foundry portal](https://ai.azure.com) and open your project. Go to **Deployments** in the left sidebar and select the serverless model you want to use.

On the deployment detail page, copy the **Target URI** — this is the value you will set as `AZURE_FOUNDRY_ENDPOINT`.

### Endpoint URL shapes

Azure AI Foundry uses two URL formats depending on how the endpoint was created:

| Type | URL shape | Notes |
|------|-----------|-------|
| **MaaS / serverless** | `https://<deployment-name>.models.ai.azure.com` | POST `/chat/completions` directly |
| **Project-scoped** | `https://<project>.services.ai.azure.com` | Same env var, different URL shape |

Both formats are supported. Use whichever URL appears on your deployment's detail page — goose sends requests to the path `/chat/completions` relative to the endpoint base URL.

### Environment variables reference

| Variable | Required | Description |
|----------|----------|-------------|
| `AZURE_FOUNDRY_ENDPOINT` | **Yes** | Full URL of the MaaS endpoint, e.g. `https://my-phi4.models.ai.azure.com` |
| `AZURE_FOUNDRY_API_KEY` | No | API key from the endpoint's deployment page; omit to use Entra ID (`DefaultAzureCredential`) |
| `AZURE_FOUNDRY_API_VERSION` | No | API version query parameter; not required for MaaS endpoints (leave unset) |

## Step 2: Authentication

Azure AI Foundry supports two authentication methods. Choose one:

### Option A: API key (recommended for quick setup)

On the deployment detail page, click **Show API key** (or navigate to **Keys and Endpoint**) and copy one of the keys.

Set `AZURE_FOUNDRY_API_KEY` to this value. goose sends the key as an `api-key` request header on every call.

### Option B: Entra ID (Microsoft identity)

Leave `AZURE_FOUNDRY_API_KEY` **unset**. goose will automatically acquire a bearer token by running:

```sh
az account get-access-token --resource https://ml.azure.com
```

and sends it as `Authorization: Bearer <token>` on every request. This requires either:

- **Azure CLI**: run `az login` before launching goose, or
- **Managed identity**: when running on an Azure-hosted VM, App Service, or container with an assigned identity — no `az login` is needed.

:::tip
Entra ID tokens are short-lived. If goose starts returning 401 errors after extended use, run `az login` again to refresh the token.
:::

## Step 3: Configure goose

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>

  1. Open goose Desktop
  2. Click the sidebar button, then **Settings** > **Models** > **Configure providers**
  3. Find **Azure AI Foundry** in the provider list and click **Configure**
  4. Enter your values:
     - **AZURE_FOUNDRY_ENDPOINT**: Paste the full endpoint URL from the Foundry portal (required)
     - **AZURE_FOUNDRY_API_KEY**: Paste your API key, or leave blank to use Entra ID authentication
  5. Click **Submit**
  6. Select a model from the list

  </TabItem>
  <TabItem value="cli" label="goose CLI">

  ### Option 1: Using `goose configure`

  ```sh
  goose configure
  ```

  1. Select **Configure Providers**
  2. Choose **Azure AI Foundry** from the list
  3. Enter your `AZURE_FOUNDRY_ENDPOINT` when prompted
  4. Enter your `AZURE_FOUNDRY_API_KEY` when prompted (press Enter to skip for Entra ID)
  5. Select a model from the list

  ### Option 2: Using environment variables

  Set the following environment variables before launching goose:

  ```sh
  export AZURE_FOUNDRY_ENDPOINT="https://my-phi4.models.ai.azure.com"
  export AZURE_FOUNDRY_API_KEY="<your-key>"   # omit this line to use Entra ID instead
  ```

  Then start goose:

  ```sh
  goose session
  ```

  :::tip
  Add these exports to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) to persist them across sessions.
  :::

  </TabItem>
</Tabs>

## Model Selection

Set the model name to match the deployment name shown in the Azure AI Foundry portal. Common examples:

| Model family | Example deployment name |
|--------------|------------------------|
| Microsoft Phi | `Phi-4` |
| Meta Llama | `Meta-Llama-3.1-70B-Instruct` |
| Mistral | `Mistral-large-2411` |
| Cohere | `Cohere-command-r-plus-08-2024` |

:::note
For single-model MaaS endpoints, the model name field is usually ignored by the Azure service — the endpoint is already bound to one specific model. goose still sends the configured model name in the request body, so setting it to the correct value keeps your configuration consistent and avoids confusion.
:::

To change the model later, use **Settings** > **Models** > **Switch models** in Desktop, or run `goose configure` in the CLI.

## Troubleshooting

### 401 Unauthorized

The request was rejected due to a missing, wrong, or expired credential.

- **API key auth**: verify that `AZURE_FOUNDRY_API_KEY` is set to the correct key shown on the deployment's **Keys and Endpoint** page in the portal.
- **Entra ID auth**: run `az login` and confirm the following command returns a token before launching goose:

  ```sh
  az account get-access-token --resource https://ml.azure.com
  ```

  If you are using a managed identity, ensure the identity has the **Azure AI Developer** role on the Foundry project.

### 404 Not Found

The endpoint URL is incorrect. Double-check `AZURE_FOUNDRY_ENDPOINT`:

1. Open the [Azure AI Foundry portal](https://ai.azure.com)
2. Navigate to your project → **Deployments** → select your model
3. Copy the **Target URI** exactly as shown — do not add trailing slashes or path segments

### Connection timeout / refused

- Ensure the endpoint URL starts with `https://` (not `http://`). Azure AI Foundry endpoints require TLS.
- Verify the deployment is in **Succeeded** state in the portal. A deployment that is still provisioning or has failed will not accept traffic.
- Check that any corporate firewall or proxy allows outbound HTTPS to `*.models.ai.azure.com` or `*.services.ai.azure.com`.

### "Unknown provider: azure_foundry"

Your version of goose does not yet include the Azure AI Foundry provider. Upgrade goose to the latest release:

```sh
# macOS / Linux (script install)
curl -fsSL https://github.com/block/goose/releases/latest/download/install.sh | sh

# Or update via your package manager
```

After upgrading, run `goose configure` again to set up the provider.

### Verify your endpoint manually

You can test connectivity and authentication with curl before configuring goose:

```sh
# Test the endpoint directly
curl -X POST "$AZURE_FOUNDRY_ENDPOINT/chat/completions" \
  -H "api-key: $AZURE_FOUNDRY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"Phi-4","messages":[{"role":"user","content":"hello"}],"max_tokens":20}'
```

A successful response returns a JSON object with a `choices` array. If you receive an error, the HTTP status code and message body will indicate whether the problem is authentication (401), routing (404), or the service itself (5xx).

For Entra ID authentication, replace the `api-key` header with a bearer token:

```sh
TOKEN=$(az account get-access-token --resource https://ml.azure.com --query accessToken -o tsv)

curl -X POST "$AZURE_FOUNDRY_ENDPOINT/chat/completions" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"Phi-4","messages":[{"role":"user","content":"hello"}],"max_tokens":20}'
```
