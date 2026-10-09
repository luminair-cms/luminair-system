# Running Log of Key Decisions

Complement to Architecture Decision Records (ADRs). Records tactical and operational decisions across milestones.

---

### 2026-10-06 — ADR-001 Accepted & Spec-Driven Testing Approach

- **Context**: Milestone 1 complete, `application` crate implemented with use cases and in-memory fakes. Decision trigger for ADR-001 reached.
- **Decisions**:
  1. **Accepted ADR-001**: Adopt hybrid testing model:
     - **Option 2 (Case Tables)** for domain state machines (`DocumentInstance` lifecycle) and decision tables (`AuthorizationService::can`).
     - **Option 3 (Markdown Given/When/Then Specs)** for application use case services (`DocumentsService`, `AccessRequestsService`).
     - **Option 4 (`proptest`)** for domain value object invariants (`AttributeId`, `DocumentTypeId`, `LocaleId`, `RegexPattern`).
  2. **Test Location**: Spec-backed tests live in `<crate>/tests/` (integration test directory) compiling against public APIs only to ensure black-box verification and prevent AI generation from reaching private internals. Legacy unit tests remain inline.
  3. **Table Representation**: Case tables implemented directly as typed Rust structures/enums in `tests/` instead of YAML/TOML, eliminating external DSL maintenance and dependencies while maintaining strict compile-time checks.
  4. **Fakes Hardening**: Repaired `FakeDocumentInstanceRepository` to honor `FieldFilter` on `find_by_type` and `count`, return `DomainError::Storage` instead of `Unauthorized` on poisoned locks, and maintain deterministic pagination sorting.
  5. **Traceability Gate**: Created `domain/tests/spec_traceability.rs` to mechanically ensure all `Approved` specification rules have corresponding tests and that tests reference valid rules.
  6. **Postponed Items**: `db_row_id` surrogate key cleanup and strict encapsulation of mutable aggregate fields are deferred to Milestone 2 (persistence implementation).

---

### 2026-10-09 — ADR-004 Revised: Asymmetric Hybrid Persistence Pattern

- **Context**: Milestone 2 preparation. Evaluation of AWS Aurora DSQL non-transactional DDL and draft invariants revealed that symmetric dual-wide-tables per document type (`articles` + `articles_published`) causes high migration friction, duplicate DDL, and rigid schema constraints on work-in-progress drafts.
- **Decisions**:
  1. **Asymmetric Pattern**: Published instances reside in a typed wide relational table (`[type]`); working drafts reside in a unified static JSONB table (`document_drafts`); revisions reside in a unified static append-only JSONB table (`document_snapshots`).
  2. **Distributed Query Defense**: Added `has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE` to the wide published table to eliminate cross-shard distributed `FULL OUTER JOIN`s in Aurora DSQL for `list_admin_headers`.
  3. **Composite Identity**: Schemas declare `display_fields: Vec<AttributeId>`, materialized at write-time into `display_values JSONB` (`IndexMap<AttributeId, String>`) and a joined text fallback `display_title VARCHAR(255)`.
  4. **CQRS Port Separation**: Split repository query operations into `find_published` (wide table), `list_admin_headers` (lightweight projections), and `find_draft_by_id` / `find_published_by_id`.
  5. **Concurrency & Drift Guards**: Added `expected_version` concurrency check and `schema_version` on drafts; infrastructure layer enforces DSQL OCC retry loops (`40001` serialization failure).
  6. **See**: [`ADR-004`](../docs/adr/ADR-004-draft-and-publish-table-pattern.md).

