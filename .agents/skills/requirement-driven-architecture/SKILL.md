---
name: requirement-driven-architecture
description: Organize application behavior as feature-owned, nested requirements. Use when implementing, modifying, refactoring, or reviewing app features.
---

# Requirement-Driven Architecture

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
- Every requirement directory must contain `requirement.md` with a behavioral contract and verifiable acceptance criteria.
- Put cross-cutting requirements under the feature that owns the shared behavior, not under every consumer.
- **All feature implementation code must live inside its owning requirement folder.** Do not place feature code directly under the feature root or in separate `src/` feature trees.
- Keep related functions together; a requirement folder may contain multiple files, and a parent requirement may implement behavior composed from children. Do not force one function or module per requirement.
- Put reusable behavior in the requirement folder that owns it; other requirements may import it.

## Requirement file

```markdown
# Email must be valid

Contract: Reject malformed email addresses before attempting login.

Acceptance criteria:
- Invalid addresses are rejected with an error.
- Valid addresses proceed to authentication.

Implementation: `validation.ts#validateEmail`
Verification: `validation.test.ts`
```

Use relative references to code and tests in this requirement folder or its descendants. Do not duplicate these mappings in comments or external trackers.

## Rules

1. Every application behavior has an owning requirement, and its implementation lives in that requirement's directory. Multiple requirements may import shared code from its owning requirement.
2. Every implemented feature, function, and hook must be **reachable from a production entry point**. Uses only in tests, stories, fixtures, or demos do not count. Exporting a symbol is not proof of reachability.
3. Every requirement has meaningful verification. Child tests do not replace integration checks needed for the parent's contract.
4. Behavioral changes update requirements, implementation, and tests in the same change. Keep contracts about observable behavior, not internal implementation details.
5. Do not add wrappers or duplicate code solely to isolate requirements. Move code to its owning folder and update imports when requirements change.

## Agent workflow

1. Find the owning feature and read affected parent/child requirements.
2. Create or revise the smallest relevant requirement(s) and acceptance criteria.
3. Put implementation in the owning requirement folder; move misplaced code there and update imports. Wire the behavior into production.
4. Update tests and references. Verify parent behavior where children interact.
5. Run relevant tests, typecheck, and production unused-code analysis. Report requirement paths changed and any checks not run or violations unresolved.

## Enforcement

When project tooling permits, configure CI/build checks to fail for implementation code outside requirement folders, missing requirement files, unresolved implementation/test references, missing verification, or production-unreachable code. Use production entry points, excluding tests and dev-only paths. Static analysis may miss dynamic registration, reflection, and dependency injection; document legitimate exceptions explicitly. A skill alone cannot guarantee compiler enforcement or prove behavioral correctness.
