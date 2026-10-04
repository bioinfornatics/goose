---
title: Scoped permissions design
sidebar_label: Scoped permissions design
---

This document defines the permission domain and conflict-resolution contract for scoped tool permissions. It describes the model only; persistence and user interfaces are intentionally separate concerns.

## Domain model

A permission rule has three parts:

- **Origin**: the rule's scope and a source identifier used for diagnostics and auditing.
- **Principal**: the operation the rule addresses. A principal targets one function, an extension, or a capability.
- **Effect**: `deny`, `ask`, or `allow`.

A permission request identifies an extension function and the capabilities declared for that invocation. Function principals match both extension and function names, extension principals match the extension name, and capability principals match any capability declared by the request.

## Scopes, lifetime, and inheritance

| Scope | Lifetime and ownership | Inheritance |
| --- | --- | --- |
| `managed` | Set by an administrator or managed distribution; lasts until the managing authority changes it | Applies to every session and workspace governed by that authority |
| `session` | Exists only for the current session | Applies to later matching calls in that session; it is not inherited by new sessions |
| `project-local` | Belongs to one user's local checkout | Applies in that workspace only and is not shared through version control |
| `project-shared` | Belongs to the project | May be inherited by users who open a trusted checkout of the project |
| `user` | Belongs to the user and lasts across sessions | Supplies the user's baseline in all workspaces |

Inheritance means that rules from all applicable scopes are candidates. It does not mean that a more local rule replaces or deletes an inherited rule. Session rules disappear with the session. A decision made for one call applies only to that call and does not create a rule. Leaving a workspace removes both project scopes from the applicable set.

Session inheritance follows execution ownership:

- A resumed session keeps its session rules; an imported session does not activate embedded rules until the import is trusted and validated.
- A fork starts a new session and does not inherit session rules by default. An explicit, visible copy operation may copy them.
- Subagents and background tasks use the applicable rules of their owning session, but cannot add, weaken, or outlive those rules.
- Changing working directories recomputes project applicability and trust. Rules from the previous workspace stop applying, and rules from the new workspace do not apply until that workspace is trusted.

Principal identity is the exact extension/function pair or capability identifier. Display names are not identities. Rules for extensions, functions, or capabilities that are absent or renamed remain inert orphaned rules; they must not fall back to a broader or similarly named principal.

## Deterministic resolution

The resolver first removes rules whose principals do not match the request. It then selects one rule using this precedence, in order:

1. **Effect:** `deny` > `ask` > `allow`, across all scopes.
2. **Scope for the same effect:** `managed` > `session` > `project-local` > `project-shared` > `user`.
3. **Specificity for the same effect and scope:** function > extension > capability.

Canonical principal and source ordering break otherwise exact ties so results do not depend on input order. These tie-breakers affect attribution only because tied rules have the same effect.

A restrictive effect always wins even when it comes from a lower-priority scope. For example, a user `deny` beats a managed `allow`; scope priority is consulted only when effects are equal.

## Workspace trust

Project-shared rules are input controlled by a workspace. They must not become active merely because a repository was opened or cloned. A caller that gathers applicable rules must include project-shared rules only after the workspace has been explicitly trusted through the host's trust mechanism.

Trusting a workspace permits its rules to participate; it does not grant their requested permissions. The normal resolver still applies, so inherited `deny` and `ask` rules remain effective. Project-local rules are local configuration, but they also apply only while operating in their associated workspace.

## Fail-closed behavior

If no rule matches, resolution is `ask`. If a caller cannot load, parse, validate, or authenticate a rule source, it must not infer `allow`; it should omit the invalid source and use `ask` or a stricter host policy. Unknown scopes, effects, principal kinds, and capabilities must likewise never widen access.

An `ask` result requires an explicit decision by the interaction layer. In a non-interactive environment where asking is impossible, the interaction layer must treat `ask` as denial rather than proceeding.

## Non-goals

This contract does not define:

- file locations, serialization formats, discovery, migration, or storage;
- CLI, desktop, or approval-prompt behavior;
- workspace trust user experience or trust-database storage;
- capability vocabulary or capability inference;
- extension identity authentication;
- policy distribution, signatures, administration, or audit-log storage;
- integration with the existing permission manager or agent loops;
- OS process, filesystem, network, or container sandboxing.

Permission resolution decides whether goose may request an operation. It is not a security sandbox and must not be treated as a substitute for operating-system enforcement.

Those systems may supply rules to the pure resolver, but they must preserve its precedence and fail-closed semantics.
