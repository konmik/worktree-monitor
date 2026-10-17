---
name: requirement-driven-architecture
description: Organize application behavior as feature-owned, nested requirements. Use when implementing, modifying, refactoring, or reviewing app features.
---

# Requirement-Driven Architecture

## What qualifies as a requirement

A requirement states the app's desired behavior or state, explicitly requested by the user. It describes what must be true, not an instruction to change something.

- Do not infer requirements from existing code or add unrequested guarantees.
- Keep only the latest applicable user intent; exclude superseded requests.
- Each requirement has one owner. Merge duplicates, including repetition between parents and children.

## Structure

Features own requirements. Requirement folders nest from broad behavior to specific obligations. Their paths are the requirement IDs.

```text
features/
  login/
    user-can-login-with-email/
      requirement.md
      email-must-be-valid/
        requirement.md
        validation.ts
        validation.test.ts
      password-must-match/
        requirement.md
        password.ts
        password.test.ts
  network-requests/
    all-requests-must-be-logged/
      requirement.md
      logging.ts
      logging.test.ts
```

- Name requirement directories as testable statements in `kebab-case`.
- Every requirement directory must contain `requirement.md` with a list of behavioral contracts.
- **All feature implementation code must live inside its owning requirement folder.**

## Requirement file

```markdown
# Email must be valid

- Invalid addresses are rejected with an error
- The rejection cause is shown to the user
```

Use relative references to code and tests in this requirement folder or its descendants. Do not duplicate these mappings in comments or external trackers.

## Rules

- Every application behavior has an owning requirement, and its implementation lives in that requirement's directory.
- Behavioral changes update requirements, implementation, and tests in the same change.
- Move code to its owning folder when requirements change.
