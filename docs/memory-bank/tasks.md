# Tasks (RPI)

This file tracks the RPI workflow for building the Memory Bank.

## Epic
- **E1**: WebApp Factory: goose (Memory Bank bootstrap + code index)

## Research
- **R1** (owner: webapp-role-pm, priority: 1): Capture repo purpose + high-level structure
  - Deliverables: `docs/memory-bank/brief.md`, `docs/memory-bank/product.md`
- **R2** (owner: webapp-role-architect, priority: 1): Identify core entry points + extension points
  - Deliverables: `docs/memory-bank/architecture.md`
- **R3** (owner: webapp-role-architect, priority: 2): Document build/test/lint/UI workflows
  - Deliverables: `docs/memory-bank/tech.md`

### Gate
- **G1**: Gate: Research Complete (blocks Plan)
  - Done when: R1–R3 are done

## Plan
- **P1** (owner: webapp-role-pm, priority: 1): Define ongoing update process + success criteria
  - Deliverables: `docs/memory-bank/context.md`

### Gate
- **G2**: Gate: Plan Approved (blocks Implement)
  - Done when: P1 done

## Implement
- **I1** (owner: webapp-role-architect, priority: 1): Create APP_CONTEXT + Memory Bank files in repo
  - Deliverables: `APP_CONTEXT.md` + Memory Bank core docs

### Gate
- **G3**: Gate: Implementation Checkpoint (blocks Verify)
  - Done when: I1 done

## Verify
- **V1** (owner: webapp-role-qa, priority: 1): Sanity check docs exist + paths resolve
  - Deliverables: `docs/memory-bank/map.md` updated if any archive files are created

### Gate
- **G4**: Gate: Verification Complete
  - Done when: V1 done
